// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Darwin
import Foundation

/// A Herdr pane a session runs in (#328): the pane and the API socket Herdr
/// puts in every pane's environment, and the server that runs the pane.
struct ClaudeHerdrPane: Equatable {
    /// The server's API socket (`HERDR_SOCKET_PATH`).
    let socket: String
    /// The pane's id, as in `w1:p2` (`HERDR_PANE_ID`). Herdr never gives a
    /// closed pane's id to another, and a moved pane's old id still finds it.
    let pane: String
    /// The named session (`HERDR_SESSION`); nil for the default one.
    let session: String?
    /// The Herdr server the session's process descends from; 0 until it is
    /// found, which is what makes the pane the session's own.
    var server: Int32 = 0
    /// The app of the terminal showing that Herdr, when one is attached.
    var outer: String?

    /// `HERDR_ENV`, `HERDR_PANE_ID`, `HERDR_SOCKET_PATH` and `HERDR_SESSION`.
    init?(environment: [String: String]) {
        guard environment["HERDR_ENV"] == "1",
              let pane = environment["HERDR_PANE_ID"], Self.isPaneID(pane),
              let socket = environment["HERDR_SOCKET_PATH"], socket.hasPrefix("/"), !socket.contains("\n"),
              socket.utf8.count < MemoryLayout.size(ofValue: sockaddr_un().sun_path) else { return nil }
        self.socket = socket
        self.pane = pane
        let session = environment["HERDR_SESSION"] ?? ""
        self.session = session.isEmpty || session.contains("\n") ? nil : session
    }

    /// `w<n>:p<n>`, the form Herdr gives pane ids.
    static func isPaneID(_ id: String) -> Bool {
        let parts = id.split(separator: ":", omittingEmptySubsequences: false)
        guard parts.count == 2, parts[0].count > 1, parts[1].count > 1,
              parts[0].first == "w", parts[1].first == "p" else { return false }
        return parts[0].dropFirst().allSatisfy(\.isASCIIDigit) && parts[1].dropFirst().allSatisfy(\.isASCIIDigit)
    }
}

private extension Character {
    var isASCIIDigit: Bool { isASCII && isNumber }
}

/// Reaches a session in Herdr through Herdr's own socket API: one JSON
/// request per line, one answer per line. Herdr keeps one focus for the
/// whole session, which every attached terminal shows, and `pane.focus`
/// switches the workspace and tab with the pane.
enum ClaudeHerdr {
    static let candidates = ["/opt/homebrew/bin/herdr", "/usr/local/bin/herdr", "/opt/local/bin/herdr"]

    /// The server's own binary first, for the line that attaches it.
    static func binary(server: Int32) -> String? {
        var path = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
        if server > 1, proc_pidpath(server, &path, UInt32(path.count)) > 0 {
            let found = String(cString: path)
            if (found as NSString).lastPathComponent == "herdr", FileManager.default.isExecutableFile(atPath: found) {
                return found
            }
        }
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    // MARK: The socket API

    /// Sends one request and reads its answer: the `result` object, or nil
    /// on an error, a socket that isn't Herdr's or no answer in time.
    static func request(_ target: ClaudeHerdrPane, _ method: String, _ params: [String: Any],
                        timeout: TimeInterval = 3) -> [String: Any]? {
        guard ownSocket(target.socket),
              let body = try? JSONSerialization.data(withJSONObject: ["id": "openbar", "method": method, "params": params]),
              let answer = exchange(target.socket, body + Data([0x0A]), timeout: timeout) else { return nil }
        return result(answer)
    }

    /// The `result` of an answer line; nil when it carries an `error`.
    static func result(_ line: Data) -> [String: Any]? {
        guard let object = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any],
              object["error"] == nil else { return nil }
        return object["result"] as? [String: Any]
    }

    /// A socket of this user's, as Herdr makes it, never anything the
    /// environment could point at instead.
    static func ownSocket(_ path: String) -> Bool {
        var info = stat()
        guard lstat(path, &info) == 0 else { return false }
        return (info.st_mode & S_IFMT) == S_IFSOCK && info.st_uid == getuid()
    }

    private static func exchange(_ path: String, _ request: Data, timeout: TimeInterval) -> Data? {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }
        defer { close(fd) }
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        var wait = timeval(tv_sec: Int(timeout), tv_usec: Int32((timeout - floor(timeout)) * 1_000_000))
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &wait, socklen_t(MemoryLayout<timeval>.size))
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &wait, socklen_t(MemoryLayout<timeval>.size))
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8)
        guard bytes.count < MemoryLayout.size(ofValue: address.sun_path) else { return nil }
        withUnsafeMutableBytes(of: &address.sun_path) { raw in
            raw.copyBytes(from: bytes)
            raw[bytes.count] = 0
        }
        let connected = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else { return nil }
        let sent = request.withUnsafeBytes { write(fd, $0.baseAddress, $0.count) }
        guard sent == request.count else { return nil }
        let deadline = Date().addingTimeInterval(timeout)
        var answer = Data()
        var chunk = [UInt8](repeating: 0, count: 16_384)
        while Date() < deadline, answer.count < 8 << 20 {
            let count = read(fd, &chunk, chunk.count)
            guard count > 0 else { break }
            answer.append(contentsOf: chunk.prefix(count))
            if let end = answer.firstIndex(of: 0x0A) { return answer.prefix(upTo: end) }
        }
        return nil
    }

    // MARK: Processes

    /// The Herdr server is `herdr server` (`herdr server --handoff-import …`
    /// after an update), which Herdr starts apart from the terminal; its
    /// clients are plain `herdr` processes on a terminal. `herdr server stop`
    /// and the like only ask a server.
    static func isServer(arguments: [String]) -> Bool {
        guard arguments.count >= 2, (arguments[0] as NSString).lastPathComponent == "herdr",
              arguments[1] == "server" else { return false }
        return arguments.count == 2 || arguments[2].hasPrefix("--")
    }

    static func isServer(pid: Int32) -> Bool {
        AgentHookProcess.name(of: pid) == "herdr" && isServer(arguments: ClaudeZellij.arguments(of: pid))
    }

    /// The Herdr server the process runs under: Herdr starts each pane's
    /// program itself, so its server is an ancestor. A tmux or zellij
    /// started in a pane runs its own server, and its sessions are its own:
    /// one met on the way up is the innermost multiplexer.
    static func server(of pid: Int32, parent: (Int32) -> Int32? = AgentHookProcess.parent(of:),
                       isServer: (Int32) -> Bool = isServer(pid:),
                       isOtherServer: (Int32) -> Bool = isOtherMultiplexer(pid:)) -> Int32? {
        var current = parent(pid)
        var steps = 0
        while let pid = current, pid > 1, steps < 64 {
            if isServer(pid) { return pid }
            if isOtherServer(pid) { return nil }
            current = parent(pid)
            steps += 1
        }
        return nil
    }

    /// A zellij or tmux server, which a pane's program never runs under
    /// when it is Herdr's.
    static func isOtherMultiplexer(pid: Int32) -> Bool {
        switch AgentHookProcess.name(of: pid) {
        case "zellij": return ClaudeZellij.isServer(arguments: ClaudeZellij.arguments(of: pid))
        case "tmux": return true
        default: return false
        }
    }

    /// The pane still runs the session: its server is still the session's own.
    static func holds(_ target: ClaudeHerdrPane, pid: Int32) -> Bool {
        target.server > 1 && server(of: pid) == target.server
    }

    // MARK: Actions

    static func send(_ steps: [ClaudeTypingStep], to target: ClaudeHerdrPane, pid: Int32) -> ClaudeReplyFailure? {
        guard holds(target, pid: pid) else { return .unreachable }
        for step in steps {
            switch step {
            case .paste(let text):
                // `send_input` pastes, bracketed when the pane asked for
                // it, into that pane alone, whichever is in front.
                guard request(target, "pane.send_input", ["pane_id": target.pane, "text": text]) != nil else {
                    return .unreachable
                }
            case .wait(let seconds):
                Thread.sleep(forTimeInterval: seconds)
            case .enter:
                guard keys(target, ["enter"]) else { return .unreachable }
            case .interrupt:
                guard keys(target, ["ctrl+c"]) else { return .unreachable }
            case .keys(let pressed):
                guard keys(target, pressed.map(\.kittyName)) else { return .unreachable }
            }
        }
        return nil
    }

    private static func keys(_ target: ClaudeHerdrPane, _ keys: [String]) -> Bool {
        request(target, "pane.send_keys", ["pane_id": target.pane, "keys": keys]) != nil
    }

    enum Focus: Equatable {
        /// The pane is focused; the terminal attached to Herdr that was used
        /// last shows it, when that terminal could be found.
        case client(ClaudeZellijClient?)
        /// No terminal is attached to the session.
        case detached
        case missing
    }

    /// Focuses the pane, its workspace and tab with it.
    static func focus(_ target: ClaudeHerdrPane, pid: Int32) -> Focus {
        guard holds(target, pid: pid),
              request(target, "pane.get", ["pane_id": target.pane]) != nil else { return .missing }
        let attached = clients(of: target)
        if attached?.isEmpty == true { return .detached }
        guard request(target, "pane.focus", ["pane_id": target.pane]) != nil else { return .missing }
        return .client(attached.flatMap(lastUsed))
    }

    /// Whether Herdr shows the pane: its focus is the pane and a terminal is
    /// attached. nil when Herdr didn't answer.
    static func focused(_ target: ClaudeHerdrPane, timeout: TimeInterval = 1) -> Bool? {
        guard let snapshot = request(target, "session.snapshot", [:], timeout: timeout)?["snapshot"] as? [String: Any]
        else { return nil }
        return focusedPane(snapshot) == target.pane
    }

    /// The focused pane of a `session.snapshot`.
    static func focusedPane(_ snapshot: [String: Any]) -> String? {
        snapshot["focused_pane_id"] as? String
    }

    /// The command line that attaches the session in a new terminal window.
    static func attachLine(_ target: ClaudeHerdrPane) -> String? {
        guard let herdr = binary(server: target.server) else { return nil }
        let arguments = target.session.map { [herdr, "session", "attach", $0] } ?? [herdr]
        return arguments.map(HomebrewCommandBuilder.shellQuote).joined(separator: " ")
    }

    // MARK: The terminals attached

    /// The terminal attached to the session that was used last.
    static func client(of target: ClaudeHerdrPane) -> ClaudeZellijClient? {
        clients(of: target).flatMap(lastUsed)
    }

    static func lastUsed(_ clients: [ClaudeZellijClient]) -> ClaudeZellijClient? {
        clients.max { ClaudeZellij.lastRead($0.tty) < ClaudeZellij.lastRead($1.tty) }
    }

    /// The terminals attached to the session, found by the unix socket each
    /// client keeps to the server, as for zellij; nil when that can't be told.
    static func clients(of target: ClaudeHerdrPane) -> [ClaudeZellijClient]? {
        guard target.server > 1 else { return nil }
        let candidates = clientProcesses()
        guard !candidates.isEmpty else { return [] }
        guard let listed = ClaudeHostCommand.run("/usr/sbin/lsof",
                                                 ["-a", "-U", "-n", "-P", "-F", "pdn", "-p",
                                                  ([target.server] + candidates).map(String.init).joined(separator: ",")],
                                                 timeout: 2) else { return nil }
        return clients(lsof: listed, server: target.server, api: target.socket).compactMap { pid in
            ClaudeSessionHosts.tty(of: pid).map { ClaudeZellijClient(pid: pid, tty: $0) }
        }
    }

    /// From `lsof -F pdn`: the processes whose socket's peer is one of the
    /// server's, the API socket left out. A `herdr pane …` asking the server
    /// from some terminal talks over that one and is no client.
    static func clients(lsof listed: String, server: Int32, api: String) -> [Int32] {
        var pid: Int32 = 0
        var device = ""
        var serverSockets: Set<String> = []
        var peers: [(pid: Int32, peer: String)] = []
        for line in listed.split(separator: "\n") {
            guard let kind = line.first else { continue }
            let value = String(line.dropFirst())
            switch kind {
            case "p": pid = Int32(value) ?? 0
            case "d": device = value
            case "n":
                if pid == server {
                    if !device.isEmpty, value != api { serverSockets.insert(device) }
                } else if value.hasPrefix("->") {
                    peers.append((pid, String(value.dropFirst(2))))
                }
            default: break
            }
        }
        var found: [Int32] = []
        for (pid, peer) in peers where serverSockets.contains(peer) && !found.contains(pid) { found.append(pid) }
        return found
    }

    /// Processes named herdr on a terminal that aren't servers.
    static func clientProcesses() -> [Int32] {
        let count = proc_listallpids(nil, 0)
        guard count > 0 else { return [] }
        var pids = [Int32](repeating: 0, count: Int(count) + 32)
        let filled = proc_listallpids(&pids, Int32(pids.count * MemoryLayout<Int32>.size))
        guard filled > 0 else { return [] }
        return pids.prefix(Int(filled)).filter { pid in
            pid > 1 && AgentHookProcess.name(of: pid) == "herdr" && ClaudeSessionHosts.tty(of: pid) != nil
                && !isServer(arguments: ClaudeZellij.arguments(of: pid))
        }
    }
}
