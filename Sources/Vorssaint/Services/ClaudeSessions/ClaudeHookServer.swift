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
    final class Connection {
        fileprivate let descriptor: Int32
        fileprivate var source: DispatchSourceRead?
        fileprivate var buffer = Data()
        fileprivate var delivered = false
        /// The hook process on the other end. The hook stops writing as soon
        /// as its event is sent, so only this tells that it is still there.
        let peer: pid_t
        /// Called on the server's queue when the hook goes away unanswered.
        var onClose: (() -> Void)?
        private(set) var isClosed = false

        fileprivate init(descriptor: Int32, peer: pid_t) {
            self.descriptor = descriptor
            self.peer = peer
        }

        /// The hook was killed, as when its session answered in the terminal
        /// and moved on, or quit.
        var peerIsGone: Bool { peer > 0 && kill(peer, 0) != 0 && errno == ESRCH }

        /// Writes the answer, if any, and ends the connection. Call on the
        /// server's queue.
        func finish(_ reply: Data? = nil) {
            guard !isClosed else { return }
            if let reply, !reply.isEmpty {
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
            close(notifying: false)
        }

        fileprivate func close(notifying: Bool) {
            guard !isClosed else { return }
            isClosed = true
            // A reading source closes the descriptor once it is cancelled;
            // without one it is closed here.
            if let source {
                source.cancel()
                self.source = nil
            } else {
                Darwin.close(descriptor)
            }
            if notifying { onClose?() }
            onClose = nil
        }
    }

    static let maximumInput = 4 << 20

    let path: String
    private let queue: DispatchQueue
    private let handler: (Data, Connection) -> Void
    private var listener: DispatchSourceRead?

    /// `handler` runs on `queue` with each complete event.
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
            source.setEventHandler { [weak self, connection] in self?.read(connection) }
            source.setCancelHandler { [connection] in if connection.isClosed { Darwin.close(client) } }
            connection.source = source
            source.resume()
        }
    }

    private func read(_ connection: Connection) {
        var chunk = [UInt8](repeating: 0, count: 64 << 10)
        while true {
            let count = Darwin.read(connection.descriptor, &chunk, chunk.count)
            if count > 0 {
                guard !connection.delivered else { continue }
                connection.buffer.append(contentsOf: chunk[0..<count])
                if connection.buffer.count > Self.maximumInput { return connection.close(notifying: true) }
                continue
            }
            if count < 0, errno == EINTR { continue }
            if count < 0, errno == EAGAIN || errno == EWOULDBLOCK { break }
            // The hook ended its side once it sent the event, and keeps
            // waiting for the answer: stop reading, keep the connection.
            connection.source?.cancel()
            connection.source = nil
            if connection.delivered { return }
            if !ClaudeHookMessage.isComplete(connection.buffer) { return connection.close(notifying: false) }
            break
        }
        guard !connection.delivered, ClaudeHookMessage.isComplete(connection.buffer) else { return }
        connection.delivered = true
        let payload = connection.buffer
        connection.buffer = Data()
        handler(payload, connection)
    }
}
