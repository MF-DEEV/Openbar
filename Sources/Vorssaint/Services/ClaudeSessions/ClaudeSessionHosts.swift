// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// Finds the app showing a session's terminal. A terminal app starts its
/// shells with its own bundle identifier in their environment, and the
/// session inherits it; the parent chain is the fallback.
enum ClaudeSessionHosts {
    static let terminal = "com.apple.Terminal"
    static let antigravity = "com.google.antigravity-ide"

    static func classify(bundle: String?, tty: String?, port: Int?) -> ClaudeSessionHost {
        switch bundle {
        case terminal?: return tty.map { .terminal(tty: $0) } ?? .app(terminal)
        case antigravity?: return .antigravity(port: port)
        case let bundle?: return .app(bundle)
        case nil: return .unknown
        }
    }

    /// The device of the process's controlling terminal, like "/dev/ttys004".
    static func tty(of pid: Int32) -> String? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size,
              info.e_tdev != UInt32(bitPattern: -1), info.e_tdev != 0,
              let name = devname(dev_t(bitPattern: info.e_tdev), S_IFCHR) else { return nil }
        return "/dev/" + String(cString: name)
    }

    /// The keys of a session's environment that tell where it runs. Nothing
    /// else in it is kept.
    static let hostKeys: Set<String> = ["__CFBundleIdentifier", "CLAUDE_CODE_SSE_PORT"]

    /// The given variables of the environment a process of this user started with.
    static func environment(of pid: Int32, keys: Set<String> = hostKeys) -> [String: String] {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > 0 else { return [:] }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0, size > MemoryLayout<Int32>.size else { return [:] }
        return parseArguments(Array(buffer.prefix(size))).filter { keys.contains($0.key) }
    }

    /// `KERN_PROCARGS2` holds the argument count, the executable path, its
    /// padding, the arguments and then the environment, all NUL-terminated.
    static func parseArguments(_ bytes: [UInt8]) -> [String: String] {
        let countSize = MemoryLayout<Int32>.size
        guard bytes.count > countSize else { return [:] }
        let argc = bytes.prefix(countSize).withUnsafeBytes { $0.loadUnaligned(as: Int32.self) }
        var index = countSize
        func skipString() { while index < bytes.count, bytes[index] != 0 { index += 1 } }
        func skipZeros() { while index < bytes.count, bytes[index] == 0 { index += 1 } }
        skipString()
        skipZeros()
        for _ in 0..<max(0, Int(argc)) {
            skipString()
            index += 1
        }
        var environment: [String: String] = [:]
        while index < bytes.count, bytes[index] != 0 {
            let start = index
            skipString()
            let entry = String(decoding: bytes[start..<index], as: UTF8.self)
            if let equals = entry.firstIndex(of: "=") {
                environment[String(entry[..<equals])] = String(entry[entry.index(after: equals)...])
            }
            index += 1
        }
        return environment
    }
}

/// The AppleScript that reaches a session's tab in Terminal, found by the
/// device its shell runs on.
enum ClaudeTerminalScripts {
    static func focus(tty: String) -> String {
        """
        tell application "Terminal"
            repeat with theWindow in windows
                repeat with theTab in tabs of theWindow
                    if tty of theTab is \(AppleScriptRunner.literal(tty)) then
                        set selected of theTab to true
                        set index of theWindow to 1
                        activate
                        return "found"
                    end if
                end repeat
            end repeat
        end tell
        return "missing"
        """
    }
}

/// Where an Antigravity session's window is: the IDE connection's lock file
/// names the folders that window has open.
enum ClaudeIDELock {
    static func folders(port: Int, homes: [URL]) -> [String] {
        for home in homes {
            let path = home.appending(path: "ide/\(port).lock").path
            guard let data = FileManager.default.contents(atPath: path), data.count < 1 << 16,
                  let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { continue }
            return json["workspaceFolders"] as? [String] ?? []
        }
        return []
    }

    /// The window title to look for: the open folder holding the session's
    /// folder, or the session's folder itself.
    static func windowHint(cwd: String, folders: [String]) -> String {
        let holding = folders.filter { cwd == $0 || cwd.hasPrefix($0.hasSuffix("/") ? $0 : $0 + "/") }
            .max { $0.count < $1.count }
        return ((holding ?? cwd) as NSString).lastPathComponent
    }
}

extension ClaudeTerminalScripts {
    /// Types a reply into the session's tab as a paste, so its line breaks
    /// stay text, and the return Terminal adds after it sends it.
    static func type(_ text: String, tty: String) -> String {
        """
        set marker to ASCII character 27
        tell application "Terminal"
            repeat with theWindow in windows
                repeat with theTab in tabs of theWindow
                    if tty of theTab is \(AppleScriptRunner.literal(tty)) then
                        do script (marker & "[200~" & \(AppleScriptRunner.literal(text)) & marker & "[201~") in theTab
                        return "sent"
                    end if
                end repeat
            end repeat
        end tell
        return "missing"
        """
    }
}

/// Talks to the Antigravity extension that shows a session's terminal and
/// types into it. Every window of the IDE listens on its own socket in
/// `~/.vorssaint/terminals`, and only the one holding the terminal answers yes.
enum ClaudeTerminalBridge {
    static func folder(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> URL {
        home.appending(path: ".vorssaint/terminals", directoryHint: .isDirectory)
    }

    static func focus(shell: Int32, folder: URL) -> Bool {
        ask(["command": "focus", "shellPid": Int(shell)], folder: folder)
    }

    static func send(_ text: String, shell: Int32, folder: URL) -> Bool {
        ask(["command": "send", "shellPid": Int(shell), "text": text], folder: folder)
    }

    /// Asks each window in turn until one says it did it.
    static func ask(_ request: [String: Any], folder: URL, timeout: TimeInterval = 2) -> Bool {
        guard var line = try? JSONSerialization.data(withJSONObject: request) else { return false }
        line.append(0x0A)
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
        for name in names where name.hasSuffix(".sock") {
            let path = folder.appending(path: name).path
            guard let reply = exchange(line, path: path, timeout: timeout),
                  let json = (try? JSONSerialization.jsonObject(with: reply)) as? [String: Any] else { continue }
            if json["ok"] as? Bool == true { return true }
        }
        return false
    }

    /// One request and its one-line answer, or nil when nobody listens.
    static func exchange(_ request: Data, path: String, timeout: TimeInterval) -> Data? {
        let descriptor = socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else { return nil }
        defer { close(descriptor) }
        var noSignal: Int32 = 1
        setsockopt(descriptor, SOL_SOCKET, SO_NOSIGPIPE, &noSignal, socklen_t(MemoryLayout<Int32>.size))
        var wait = timeval(tv_sec: Int(timeout), tv_usec: Int32((timeout - floor(timeout)) * 1_000_000))
        setsockopt(descriptor, SOL_SOCKET, SO_RCVTIMEO, &wait, socklen_t(MemoryLayout<timeval>.size))
        setsockopt(descriptor, SOL_SOCKET, SO_SNDTIMEO, &wait, socklen_t(MemoryLayout<timeval>.size))
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
                connect(descriptor, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else { return nil }
        let written = request.withUnsafeBytes { write(descriptor, $0.baseAddress, $0.count) }
        guard written == request.count else { return nil }
        var reply = Data()
        var chunk = [UInt8](repeating: 0, count: 4096)
        while reply.count < 1 << 16 {
            let count = read(descriptor, &chunk, chunk.count)
            guard count > 0 else { break }
            reply.append(contentsOf: chunk[0..<count])
            if reply.contains(0x0A) { break }
        }
        return reply.isEmpty ? nil : reply
    }

    /// The shell a session runs in, which the IDE knows its terminal by.
    static func shell(of pid: Int32) -> Int32? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size, info.pbi_ppid > 1 else { return nil }
        return Int32(info.pbi_ppid)
    }
}
