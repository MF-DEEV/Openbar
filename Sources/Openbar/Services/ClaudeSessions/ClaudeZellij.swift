// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Darwin
import Foundation

/// A zellij pane a session runs in (#235): the session and pane zellij names
/// in every pane's environment, and the server that runs the pane.
struct ClaudeZellijPane: Equatable {
    let session: String
    /// The terminal pane's id, as in `terminal_3`.
    let pane: Int
    /// The zellij server the session's process descends from; 0 until it is
    /// found, which is what makes the pane the session's own.
    var server: Int32 = 0
    /// The app of the terminal showing that zellij, when one is attached.
    var outer: String?

    /// `ZELLIJ_SESSION_NAME` and `ZELLIJ_PANE_ID` ("3").
    init?(session: String?, pane: String?) {
        guard let session, !session.isEmpty, !session.contains("\n"),
              let pane, !pane.isEmpty, pane.allSatisfy(\.isASCII), let id = Int(pane), id >= 0 else { return nil }
        self.session = session
        self.pane = id
    }

    var paneID: String { "terminal_\(pane)" }
}

/// A terminal attached to a zellij session, which Open brings forward.
struct ClaudeZellijClient: Equatable {
    let pid: Int32
    let tty: String
}

/// One pane from `zellij action list-panes --json`.
struct ClaudeZellijListedPane: Equatable {
    let id: Int
    let tab: Int
    let focused: Bool
}

/// Reaches a session in zellij through zellij's own command line. Every
/// action from outside a session goes to the client that was used last, the
/// way tmux's most active client does; `focus-pane-id` doesn't change tabs,
/// so the tab is selected first.
enum ClaudeZellij {
    static let candidates = ["/opt/homebrew/bin/zellij", "/usr/local/bin/zellij", "/opt/local/bin/zellij"]

    /// The server's own binary first: client and server have to be the same
    /// version, or the client can't reach the session at all.
    static func binary(server: Int32) -> String? {
        var path = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
        if server > 1, proc_pidpath(server, &path, UInt32(path.count)) > 0 {
            let found = String(cString: path)
            if (found as NSString).lastPathComponent == "zellij", FileManager.default.isExecutableFile(atPath: found) {
                return found
            }
        }
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    @discardableResult
    static func run(_ target: ClaudeZellijPane, _ arguments: [String], input: String? = nil,
                    timeout: TimeInterval = 4) -> String? {
        guard let zellij = binary(server: target.server) else { return nil }
        for name in names(target) {
            if let output = ClaudeHostCommand.run(zellij, ["--session", name, "action"] + arguments,
                                                  input: input, timeout: timeout) {
                remember(name, server: target.server)
                return output
            }
        }
        return nil
    }

    /// What `--session` takes for the pane's session, the one that worked
    /// last first. zellij finds a session by its name through the metadata it
    /// keeps in the user's cache folder; with another cache folder (the app
    /// started with another HOME or XDG_CACHE_HOME than the shell) only the
    /// name of the server's socket, a hash, finds it.
    static func names(_ target: ClaudeZellijPane) -> [String] {
        var names = [target.session]
        if let socket = socketName(arguments: arguments(of: target.server)), socket != target.session {
            names.append(socket)
        }
        if let known = lock.withLock({ working[target.server] }), let index = names.firstIndex(of: known), index > 0 {
            names.swapAt(0, index)
        }
        return names
    }

    /// The last part of the path after `--server`: zellij names the socket
    /// with a hash of the session.
    static func socketName(arguments: [String]) -> String? {
        guard let index = arguments.firstIndex(of: "--server"), index + 1 < arguments.count else { return nil }
        let name = (arguments[index + 1] as NSString).lastPathComponent
        return name.isEmpty ? nil : name
    }

    private static let lock = NSLock()
    private static var working: [Int32: String] = [:]

    private static func remember(_ name: String, server: Int32) {
        lock.withLock { working[server] = name }
    }

    // MARK: Processes

    /// The command line a process of this user started with.
    static func arguments(of pid: Int32) -> [String] {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > 0 else { return [] }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0 else { return [] }
        return parseArguments(Array(buffer.prefix(size)))
    }

    /// The arguments part of `KERN_PROCARGS2`: the count, the executable
    /// path, its padding, then each argument, all NUL-terminated.
    static func parseArguments(_ bytes: [UInt8]) -> [String] {
        let countSize = MemoryLayout<Int32>.size
        guard bytes.count > countSize else { return [] }
        let argc = bytes.prefix(countSize).withUnsafeBytes { $0.loadUnaligned(as: Int32.self) }
        var index = countSize
        while index < bytes.count, bytes[index] != 0 { index += 1 }
        while index < bytes.count, bytes[index] == 0 { index += 1 }
        var arguments: [String] = []
        for _ in 0..<max(0, Int(argc)) where index < bytes.count {
            let start = index
            while index < bytes.count, bytes[index] != 0 { index += 1 }
            arguments.append(String(decoding: bytes[start..<index], as: UTF8.self))
            index += 1
        }
        return arguments
    }

    /// A zellij server is `zellij --server <socket>`; its clients are plain
    /// `zellij` processes on a terminal.
    static func isServer(arguments: [String]) -> Bool {
        arguments.count >= 2 && (arguments[0] as NSString).lastPathComponent == "zellij" && arguments.contains("--server")
    }

    /// The zellij server the process runs under: zellij starts each pane's
    /// program itself, so its server is an ancestor, and the nearest one is
    /// the innermost. A tmux or zellij started inside a pane runs its own
    /// server apart from it, so a session in that one has no zellij above it.
    static func server(of pid: Int32, parent: (Int32) -> Int32? = AgentHookProcess.parent(of:),
                       isServer: (Int32) -> Bool = isServer(pid:)) -> Int32? {
        var current = parent(pid)
        var steps = 0
        while let pid = current, pid > 1, steps < 64 {
            if isServer(pid) { return pid }
            current = parent(pid)
            steps += 1
        }
        return nil
    }

    static func isServer(pid: Int32) -> Bool {
        AgentHookProcess.name(of: pid) == "zellij" && isServer(arguments: arguments(of: pid))
    }

    /// The pane still runs the session: its server is still the session's own.
    static func holds(_ target: ClaudeZellijPane, pid: Int32) -> Bool {
        target.server > 1 && server(of: pid) == target.server
    }

    // MARK: Reading zellij's answers

    static func panes(_ json: String) -> [ClaudeZellijListedPane] {
        guard let list = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [[String: Any]] else { return [] }
        return list.compactMap { pane in
            guard (pane["is_plugin"] as? Bool) == false, let id = pane["id"] as? Int,
                  let tab = pane["tab_id"] as? Int else { return nil }
            return ClaudeZellijListedPane(id: id, tab: tab, focused: pane["is_focused"] as? Bool ?? false)
        }
    }

    /// `list-clients`: a header, then each client's id and focused pane.
    static func clientPanes(_ listed: String) -> [String] {
        listed.split(separator: "\n").dropFirst().compactMap { line in
            let fields = line.split(separator: " ", omittingEmptySubsequences: true)
            guard fields.count >= 2, Int(fields[0]) != nil else { return nil }
            return String(fields[1])
        }
    }

    // MARK: Actions

    static func send(_ steps: [ClaudeTypingStep], to target: ClaudeZellijPane, pid: Int32) -> ClaudeReplyFailure? {
        guard holds(target, pid: pid) else { return .unreachable }
        for step in steps {
            switch step {
            case .paste(let text):
                // A bracketed paste into that pane alone, whichever is in front.
                guard run(target, ["paste", "--pane-id", target.paneID, text]) != nil else { return .unreachable }
            case .wait(let seconds):
                Thread.sleep(forTimeInterval: seconds)
            case .enter:
                guard run(target, ["send-keys", "--pane-id", target.paneID, "Enter"]) != nil else { return .unreachable }
            case .interrupt:
                guard run(target, ["send-keys", "--pane-id", target.paneID, "Ctrl c"]) != nil else { return .unreachable }
            case .keys(let keys):
                guard run(target, ["send-keys", "--pane-id", target.paneID] + keys.map(\.multiplexerName)) != nil else {
                    return .unreachable
                }
            }
        }
        return nil
    }

    enum Focus: Equatable {
        /// The pane is selected; the terminal of the client used last shows
        /// it, when that terminal could be found.
        case client(ClaudeZellijClient?)
        /// No terminal is attached to the session.
        case detached
        case missing
    }

    /// Selects the pane's tab and the pane in the client used last.
    static func focus(_ target: ClaudeZellijPane, pid: Int32) -> Focus {
        guard holds(target, pid: pid),
              let listed = run(target, ["list-panes", "--json"]),
              let pane = panes(listed).first(where: { $0.id == target.pane }) else { return .missing }
        guard let clients = run(target, ["list-clients"]) else { return .missing }
        if clientPanes(clients).isEmpty { return .detached }
        guard run(target, ["go-to-tab-by-id", String(pane.tab)]) != nil else { return .missing }
        // It fails with "already focused" when it is: list-panes already
        // told the pane is there.
        run(target, ["focus-pane-id", target.paneID])
        return .client(client(of: target))
    }

    /// Whether the pane is the one zellij shows: every attached client has it
    /// focused (any client then is the one in front), or it is hidden
    /// because none has; nil when only some have, since which terminal shows
    /// which client can't be told.
    static func focusedEverywhere(_ target: ClaudeZellijPane, timeout: TimeInterval = 1) -> Bool? {
        guard let listed = run(target, ["list-clients"], timeout: timeout) else { return nil }
        return shown(clientPanes(listed), pane: target.paneID)
    }

    static func shown(_ focused: [String], pane: String) -> Bool? {
        guard !focused.isEmpty else { return false }
        let mine = focused.filter { $0 == pane }.count
        if mine == 0 { return false }
        return mine == focused.count ? true : nil
    }

    /// The command line that attaches the session in a new terminal window.
    static func attachLine(_ target: ClaudeZellijPane) -> String? {
        guard let zellij = binary(server: target.server) else { return nil }
        return [zellij, "attach", target.session].map(HomebrewCommandBuilder.shellQuote).joined(separator: " ")
    }

    // MARK: The terminals attached

    /// The terminal attached to the session that was used last, found by the
    /// unix socket each client keeps to the server; its device is read last
    /// when the person typed into it.
    static func client(of target: ClaudeZellijPane) -> ClaudeZellijClient? {
        guard target.server > 1 else { return nil }
        let candidates = clientProcesses()
        guard !candidates.isEmpty,
              let listed = ClaudeHostCommand.run("/usr/sbin/lsof",
                                                 ["-a", "-U", "-n", "-P", "-F", "pdn", "-p",
                                                  ([target.server] + candidates).map(String.init).joined(separator: ",")],
                                                 timeout: 2) else { return nil }
        let attached = clients(lsof: listed, server: target.server)
        return attached
            .compactMap { pid -> (ClaudeZellijClient, Double)? in
                guard let tty = ClaudeSessionHosts.tty(of: pid) else { return nil }
                return (ClaudeZellijClient(pid: pid, tty: tty), lastRead(tty))
            }
            .max { $0.1 < $1.1 }?.0
    }

    /// Processes named zellij on a terminal that aren't servers.
    static func clientProcesses() -> [Int32] {
        let count = proc_listallpids(nil, 0)
        guard count > 0 else { return [] }
        var pids = [Int32](repeating: 0, count: Int(count) + 32)
        let filled = proc_listallpids(&pids, Int32(pids.count * MemoryLayout<Int32>.size))
        guard filled > 0 else { return [] }
        return pids.prefix(Int(filled)).filter { pid in
            pid > 1 && AgentHookProcess.name(of: pid) == "zellij" && ClaudeSessionHosts.tty(of: pid) != nil
                && !isServer(arguments: arguments(of: pid))
        }
    }

    /// From `lsof -F pdn`: the processes whose socket's peer is one of the
    /// server's sockets.
    static func clients(lsof listed: String, server: Int32) -> [Int32] {
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
                    if !device.isEmpty { serverSockets.insert(device) }
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

    /// When a device was last read, which is when someone typed into it.
    static func lastRead(_ tty: String) -> Double {
        var info = stat()
        guard stat(tty, &info) == 0 else { return 0 }
        return Double(info.st_atimespec.tv_sec) + Double(info.st_atimespec.tv_nsec) / 1e9
    }
}
