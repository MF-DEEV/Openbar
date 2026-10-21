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

    /// The environment a process of this user started with.
    static func environment(of pid: Int32) -> [String: String] {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > 0 else { return [:] }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0, size > MemoryLayout<Int32>.size else { return [:] }
        return parseArguments(Array(buffer.prefix(size)))
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
