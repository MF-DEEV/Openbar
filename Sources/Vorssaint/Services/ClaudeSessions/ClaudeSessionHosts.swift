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

    /// Where a session runs, from the keys of its environment that tell
    /// (`hostKeys`), its device and the app macOS holds responsible for it.
    /// A multiplexer comes first, as the innermost terminal; then the
    /// terminal's own `TERM_PROGRAM`; then the bundle identifier it inherited.
    static func classify(bundle: String?, tty: String?, port: Int?, environment: [String: String] = [:],
                         owner: String? = nil) -> ClaudeSessionHost {
        let program = environment["TERM_PROGRAM"]
        if program == nil || program == "tmux", let tmux = environment["TMUX"], let pane = environment["TMUX_PANE"],
           let target = ClaudeTmuxPane(tmux: tmux, pane: pane) {
            return .tmux(target)
        }
        // kitty and Alacritty leave a TERM_PROGRAM they inherited alone, so
        // one started from an editor's terminal still says "vscode"; the
        // TERM every terminal sets for itself tells them apart.
        let term = environment["TERM"]
        if term == "xterm-kitty" || program == "kitty", let window = environment["KITTY_WINDOW_ID"].flatMap(Int.init) {
            return .kitty(window: window, socket: environment["KITTY_LISTEN_ON"].flatMap { $0.isEmpty ? nil : $0 })
        }
        if term == "alacritty" { return .app(ClaudeTerminalApps.alacritty) }
        if term == "xterm-ghostty" { return tty.map { .ghostty(tty: $0) } ?? .app(ClaudeTerminalApps.ghostty) }
        if program == "vscode" {
            // An editor started from a terminal, or from another editor's
            // terminal, inherits that one's identifier; the app responsible
            // for the process knows better.
            if let ide = ClaudeIDE.find(owner) ?? ClaudeIDE.find(bundle) { return .ide(bundle: ide.bundle, port: port) }
            return (bundle ?? owner).map { .app($0) } ?? .unknown
        }
        if program == nil, let window = environment["KITTY_WINDOW_ID"].flatMap(Int.init) {
            return .kitty(window: window, socket: environment["KITTY_LISTEN_ON"].flatMap { $0.isEmpty ? nil : $0 })
        }
        switch ClaudeTerminalApps.bundle(program: program) ?? bundle ?? owner {
        case terminal?: return tty.map { .terminal(tty: $0) } ?? .app(terminal)
        case ClaudeTerminalApps.iterm?: return tty.map { .iterm(tty: $0) } ?? .app(ClaudeTerminalApps.iterm)
        case ClaudeTerminalApps.ghostty?: return tty.map { .ghostty(tty: $0) } ?? .app(ClaudeTerminalApps.ghostty)
        case ClaudeTerminalApps.wezterm?:
            guard let pane = environment["WEZTERM_PANE"].flatMap(Int.init) else { return .app(ClaudeTerminalApps.wezterm) }
            return .wezterm(pane: pane, socket: environment["WEZTERM_UNIX_SOCKET"].flatMap { $0.isEmpty ? nil : $0 })
        case let bundle? where ClaudeIDE.find(bundle) != nil: return .ide(bundle: bundle, port: port)
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

    /// The working folder of a process of this user.
    static func cwd(of pid: Int32) -> String? {
        var info = proc_vnodepathinfo()
        let size = Int32(MemoryLayout<proc_vnodepathinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &info, size) == size else { return nil }
        let path = withUnsafeBytes(of: info.pvi_cdir.vip_path) { raw in
            String(decoding: raw.prefix { $0 != 0 }, as: UTF8.self)
        }
        return path.isEmpty ? nil : path
    }

    /// Codex's terminal UIs: processes named `codex` with a controlling
    /// terminal, by working folder. The shared app-server has no terminal,
    /// so it is left out.
    static func codexTerminals() -> [(pid: Int32, cwd: String)] {
        let count = proc_listallpids(nil, 0)
        guard count > 0 else { return [] }
        var pids = [Int32](repeating: 0, count: Int(count) + 32)
        let filled = proc_listallpids(&pids, Int32(pids.count * MemoryLayout<Int32>.size))
        guard filled > 0 else { return [] }
        return pids.prefix(Int(filled)).compactMap { pid in
            guard pid > 0 else { return nil }
            // proc_name answers nothing into a buffer shorter than the longest name (2 × MAXCOMLEN).
            var name = [CChar](repeating: 0, count: 2 * Int(MAXCOMLEN) + 1)
            guard proc_name(pid, &name, UInt32(name.count)) > 0, String(cString: name) == "codex",
                  tty(of: pid) != nil, let cwd = cwd(of: pid) else { return nil }
            return (pid, cwd)
        }
    }

    /// The terminal UI showing a Codex thread that runs in `cwd`: the one
    /// there, else the only one running at all; with several and none in
    /// that folder alone, none (Open then has nothing to guess from).
    /// `others` are the folders of the other Codex sessions listed: the only
    /// terminal never stands in for this one when it runs in one of theirs.
    static func codexTerminal(cwd: String, among terminals: [(pid: Int32, cwd: String)], others: [String] = []) -> Int32? {
        let standard = { (path: String) in URL(fileURLWithPath: path).standardizedFileURL.resolvingSymlinksInPath().path }
        let wanted = standard(cwd)
        let here = terminals.filter { standard($0.cwd) == wanted }
        if here.count == 1 { return here[0].pid }
        guard here.isEmpty, terminals.count == 1 else { return nil }
        let claimed = Set(others.map(standard))
        return claimed.contains(standard(terminals[0].cwd)) ? nil : terminals[0].pid
    }

    /// The keys of a session's environment that tell where it runs. Nothing
    /// else in it is kept.
    static let hostKeys: Set<String> = [
        "__CFBundleIdentifier", "CLAUDE_CODE_SSE_PORT", "TERM", "TERM_PROGRAM", "TMUX", "TMUX_PANE",
        "WEZTERM_PANE", "WEZTERM_UNIX_SOCKET", "KITTY_WINDOW_ID", "KITTY_LISTEN_ON",
    ]

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

    /// The folder open in the session's window that holds the session's
    /// folder, the deepest one when several do; nil when none is open there.
    static func openFolder(cwd: String, folders: [String]) -> String? {
        folders.filter { cwd == $0 || cwd.hasPrefix($0.hasSuffix("/") ? $0 : $0 + "/") }.max { $0.count < $1.count }
    }

    /// The window title to look for: the open folder holding the session's
    /// folder, or the session's folder itself.
    static func windowHint(cwd: String, folders: [String]) -> String {
        ((openFolder(cwd: cwd, folders: folders) ?? cwd) as NSString).lastPathComponent
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

    /// The time Claude Code takes to read pasted images before the text.
    static func imageSettle(_ count: Int) -> Double { 1.2 + 0.3 * Double(count) }

    /// Types images and a reply. Claude Code attaches an image when a paste
    /// is only its path, and reads it a moment later; Terminal adds a return
    /// after each `do script`. So the paths go first with a paste left open,
    /// which the first return joins as a line break, and after a pause the
    /// text closes that paste and its return sends the whole reply.
    static func type(_ text: String, images: [String], tty: String) -> String {
        let pastes = images.map { "marker & \"[200~\" & \(AppleScriptRunner.literal($0)) & marker & \"[201~\"" }
        let opening = (pastes + ["marker & \"[200~\""]).joined(separator: " & ")
        return """
        set marker to ASCII character 27
        tell application "Terminal"
            repeat with theWindow in windows
                repeat with theTab in tabs of theWindow
                    if tty of theTab is \(AppleScriptRunner.literal(tty)) then
                        do script (\(opening)) in theTab
                        delay \(imageSettle(images.count))
                        do script (\(AppleScriptRunner.literal(text)) & marker & "[201~") in theTab
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

    /// Images and a reply. A window still running an extension from before
    /// images, until it reloads, says it does not know the command; the
    /// reply then names the images' files, which Claude reads itself.
    static func send(_ text: String, images: [String], named: [String]? = nil, shell: Int32, folder: URL) -> ClaudeReplyFailure? {
        let request: [String: Any] = ["command": "send-images", "shellPid": Int(shell), "text": text, "images": images]
        let replies = answers(request, folder: folder, timeout: 4 + ClaudeTerminalScripts.imageSettle(images.count))
        if replies.contains(where: { $0["ok"] as? Bool == true }) { return nil }
        guard replies.contains(where: { $0["error"] as? String == "unknown" }) else { return .unreachable }
        return send(naming(named ?? images, in: text), shell: shell, folder: folder) ? nil : .unreachable
    }

    /// A reply that names its images' files instead of attaching them.
    static func naming(_ images: [String], in text: String) -> String {
        let lines = images.map { "[Image: \($0)]" }.joined(separator: "\n")
        return text.isEmpty ? lines : text + "\n\n" + lines
    }

    /// Asks each window in turn until one says it did it.
    static func ask(_ request: [String: Any], folder: URL, timeout: TimeInterval = 4) -> Bool {
        answers(request, folder: folder, timeout: timeout).contains { $0["ok"] as? Bool == true }
    }

    /// The windows' answers, up to the first that did it.
    static func answers(_ request: [String: Any], folder: URL, timeout: TimeInterval) -> [[String: Any]] {
        guard var line = try? JSONSerialization.data(withJSONObject: request) else { return [] }
        line.append(0x0A)
        var replies: [[String: Any]] = []
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
        for name in names where name.hasSuffix(".sock") {
            let path = folder.appending(path: name).path
            guard let reply = exchange(line, path: path, timeout: timeout),
                  let json = (try? JSONSerialization.jsonObject(with: reply)) as? [String: Any] else { continue }
            replies.append(json)
            if json["ok"] as? Bool == true { break }
        }
        return replies
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
        guard let parent = parent(of: pid) else { return nil }
        // Codex installed with npm runs its binary under a `node` launcher;
        // the shell is the one above it.
        var name = [CChar](repeating: 0, count: 2 * Int(MAXCOMLEN) + 1)
        if proc_name(parent, &name, UInt32(name.count)) > 0, String(cString: name) == "node",
           let shell = self.parent(of: parent) {
            return shell
        }
        return parent
    }

    private static func parent(of pid: Int32) -> Int32? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size, info.pbi_ppid > 1 else { return nil }
        return Int32(info.pbi_ppid)
    }
}
