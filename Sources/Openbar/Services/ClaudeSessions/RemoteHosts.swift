// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// A machine the person reaches over SSH, named by its alias in their own
/// `~/.ssh/config`. Openbar runs the system `ssh` with their keys and agent
/// and never asks for or keeps a password (issue #189).
struct RemoteHost: Codable, Equatable, Identifiable, Hashable {
    let alias: String
    var id: String { alias }

    /// An alias `ssh` takes as a destination and never as an option: no
    /// leading dash, no spaces, quotes or control characters.
    static func isValid(_ alias: String) -> Bool {
        guard !alias.isEmpty, alias.utf8.count <= 255, alias.first != "-" else { return false }
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "._-@:[]%"))
        return alias.unicodeScalars.allSatisfy { $0.isASCII && allowed.contains($0) }
    }
}

/// Where the hosts and the switch for them are kept.
enum RemoteHostPrefs {
    /// Off by default: nothing is reached or installed until the person turns it on.
    static func isEnabled(in defaults: UserDefaults = .standard) -> Bool {
        defaults.bool(forKey: DefaultsKey.notchSessionsRemoteHostsEnabled)
    }

    static func hosts(in defaults: UserDefaults = .standard) -> [RemoteHost] {
        guard let text = defaults.string(forKey: DefaultsKey.notchSessionsRemoteHosts),
              let hosts = try? JSONDecoder().decode([RemoteHost].self, from: Data(text.utf8)) else { return [] }
        var seen: Set<String> = []
        return hosts.filter { RemoteHost.isValid($0.alias) && seen.insert($0.alias).inserted }
    }

    static func save(_ hosts: [RemoteHost], in defaults: UserDefaults = .standard) {
        let data = (try? JSONEncoder().encode(hosts)) ?? Data("[]".utf8)
        defaults.set(String(decoding: data, as: UTF8.self), forKey: DefaultsKey.notchSessionsRemoteHosts)
    }

    /// The hosts `ssh` will be asked to reach: those listed, while the switch is on.
    static func active(in defaults: UserDefaults = .standard) -> [RemoteHost] {
        isEnabled(in: defaults) ? hosts(in: defaults) : []
    }

    /// Aliases from `~/.ssh/config` and the files it includes from `~/.ssh`,
    /// offered when adding a host. Patterns are left out.
    static func configAliases(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [String] {
        var aliases: [String] = []
        var seen: Set<String> = []
        var files = [home.appending(path: ".ssh/config")]
        var read: Set<String> = []
        while let file = files.first {
            files.removeFirst()
            guard read.insert(file.path).inserted, read.count < 16,
                  let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            for line in text.split(whereSeparator: \.isNewline) {
                let words = line.split(whereSeparator: { $0 == " " || $0 == "\t" || $0 == "=" }).map(String.init)
                guard let key = words.first?.lowercased() else { continue }
                if key == "include" {
                    for pattern in words.dropFirst() where !pattern.contains("*") {
                        let expanded = pattern.hasPrefix("~/") ? home.appending(path: String(pattern.dropFirst(2)))
                            : pattern.hasPrefix("/") ? URL(fileURLWithPath: pattern) : home.appending(path: ".ssh/" + pattern)
                        files.append(expanded)
                    }
                } else if key == "host" {
                    for alias in words.dropFirst() where !alias.contains("*") && !alias.contains("?") && !alias.hasPrefix("!")
                        && RemoteHost.isValid(alias) && seen.insert(alias).inserted {
                        aliases.append(alias)
                    }
                }
            }
        }
        return aliases
    }
}

/// How a host stands, as Settings and the island show it.
enum RemoteHostState: Equatable {
    case connecting
    case connected(RemoteHostInfo)
    /// Not reachable now; tried again at `retry`.
    case disconnected(reason: String, retry: Date?)

    var isConnected: Bool { if case .connected = self { return true } else { return false } }
}

/// What the probe found on a host.
struct RemoteHostInfo: Equatable {
    var home: String
    var system: String
    /// The program the hooks write with, and its path; nil when the host has none.
    var client: RemoteHookClient?
    var tmux: Bool
    var claude: Bool
    /// The hooks are in the host's Claude Code settings.
    var hooksInstalled: Bool? = nil
}

enum RemoteHookClient: Equatable {
    case nc(String)
    case socat(String)
    case python3(String)
}

/// Waits between tries to reach a host that went away.
enum RemoteBackoff {
    static func delay(attempt: Int) -> TimeInterval {
        min(60, pow(2, Double(max(1, min(attempt, 6)))))
    }
}

/// The `ssh` command lines, and the loader every one of them runs: the login
/// shell may be fish or zsh, so all it is given is one `sh` started with
/// single quotes, which every shell reads the same way.
enum RemoteSSH {
    static let binary = "/usr/bin/ssh"

    static let loader = #"exec sh -c 'IFS= read -r s; eval "$(printf %s "$s" | base64 -d)"'"#

    /// Options every connection uses: never a prompt, so a host that needs a
    /// password or an unknown key fails instead of hanging.
    static func options(timeout: Int = 10) -> [String] {
        ["-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=\(timeout)", "-o", "ServerAliveInterval=10",
         "-o", "ServerAliveCountMax=3", "-o", "LogLevel=ERROR"]
    }

    static func oneShot(_ alias: String) -> [String] {
        options() + ["-o", "ClearAllForwardings=yes", "--", alias, loader]
    }

    /// The connection that stays: the host's hook socket forwarded to the
    /// island's, and the agent answering on standard input and output.
    static func link(_ alias: String, remoteSocket: String, localSocket: String) -> [String] {
        options() + ["-o", "ExitOnForwardFailure=yes", "-o", "ControlMaster=no", "-o", "ControlPath=none",
                     "-R", "\(remoteSocket):\(localSocket)", "--", alias, loader]
    }

    /// The name of the host's socket for this build of the app.
    static func socketName(bundle: String) -> String {
        bundle.filter { $0.isLetter || $0.isNumber || $0 == "." || $0 == "-" } + ".sock"
    }

    /// The first lines sent to the loader: the script, then the mode.
    static func preamble(_ mode: String) -> String? {
        guard let script = RemoteAgentScript.text else { return nil }
        return Data(script.utf8).base64EncodedString() + "\n" + mode + "\n"
    }

    /// The last line of what `ssh` printed on standard error, for Settings.
    static func reason(_ stderr: String, status: Int32) -> String {
        let line = stderr.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }
            .last { !$0.isEmpty }
        return line.map { String($0.prefix(160)) } ?? "ssh exited with \(status)"
    }
}

/// The script the host runs, shipped with the app (`Resources/remote-agent.sh`).
enum RemoteAgentScript {
    static let text: String? = {
        if let url = Bundle.main.url(forResource: "remote-agent", withExtension: "sh"),
           let text = try? String(contentsOf: url, encoding: .utf8) { return text }
        // Tests run from the repository.
        return try? String(contentsOf: URL(fileURLWithPath: "Resources/remote-agent.sh"), encoding: .utf8)
    }()
}

// MARK: Answers

/// What the agent answered: its tagged lines, without the closing ".".
enum RemoteAnswer {
    static func decode(_ field: String) -> String? {
        guard field != "-", let data = Data(base64Encoded: field) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func data(_ field: String) -> Data? { field == "-" ? nil : Data(base64Encoded: field) }

    static func encode(_ text: String) -> String { Data(text.utf8).base64EncodedString() }

    static func probe(_ lines: [String]) -> RemoteHostInfo? {
        var info = RemoteHostInfo(home: "", system: "", client: nil, tmux: false, claude: false)
        for line in lines {
            let words = line.split(separator: " ").map(String.init)
            guard let tag = words.first else { continue }
            switch tag {
            case "H": info.home = words.count > 1 ? decode(words[1]) ?? "" : ""
            case "U": info.system = words.dropFirst().joined(separator: " ")
            case "C" where words.count >= 3:
                guard let path = decode(words[2]), path.hasPrefix("/") else { continue }
                switch words[1] {
                case "nc": info.client = .nc(path)
                case "socat": info.client = .socat(path)
                case "python3": info.client = .python3(path)
                default: break
                }
            case "M": info.tmux = words.count > 1 && words[1] == "yes"
            case "A": info.claude = words.count > 1 && words[1] == "yes"
            case "I": info.hooksInstalled = words.count > 1 && words[1] == "yes"
            case "E": return nil
            default: break
            }
        }
        return info.home.hasPrefix("/") ? info : nil
    }

    /// One look at the host's running sessions.
    static func snapshot(_ lines: [String], alias: String) -> RemoteSnapshot {
        var snapshot = RemoteSnapshot()
        for line in lines {
            let words = line.split(separator: " ").map(String.init)
            guard let tag = words.first else { continue }
            switch tag {
            case "R" where words.count >= 7:
                guard let pid = Int32(words[1]), let json = data(words[2]),
                      var record = ClaudeSessionRegistryReader.record(json, remote: true), record.pid == pid else { continue }
                record.configHome = decode(words[3])
                var pane: RemoteTmuxPane?
                if let socket = decode(words[4]), let tty = decode(words[6]), words[5].hasPrefix("%") {
                    pane = RemoteTmuxPane(socket: socket, pane: words[5], tty: tty)
                }
                record.host = .remote(alias: alias, tmux: pane != nil)
                snapshot.records.append(record)
            case "J" where words.count >= 3:
                if let path = decode(words[2]) { snapshot.transcripts[words[1]] = path }
            case "T" where words.count >= 3:
                guard let path = decode(words[1]), let offset = UInt64(words[2]) else { continue }
                let bytes = words.count > 3 ? Data(base64Encoded: words[3]) ?? Data() : Data()
                snapshot.chunks.append(RemoteChunk(path: path, offset: offset, bytes: bytes))
            case "G" where words.count >= 2:
                if let path = decode(words[1]) { snapshot.gone.append(path) }
            default: break
            }
        }
        return snapshot
    }

    /// nil when the agent did it, the failure otherwise.
    static func outcome(_ lines: [String]?) -> ClaudeReplyFailure? {
        guard let lines else { return .unreachable }
        if lines.contains("O") { return nil }
        return lines.contains("F waiting") ? .waiting : .unreachable
    }
}

struct RemoteTmuxPane: Equatable {
    let socket: String
    let pane: String
    let tty: String
}

struct RemoteChunk: Equatable {
    let path: String
    let offset: UInt64
    let bytes: Data
}

struct RemoteSnapshot: Equatable {
    var records: [ClaudeSessionRecord] = []
    /// Session ID to the transcript found for it.
    var transcripts: [String: String] = [:]
    var chunks: [RemoteChunk] = []
    var gone: [String] = []
}

/// Reads a remote transcript in the pieces the agent sends: a piece may end
/// inside a line, which waits for the next one.
struct RemoteTranscriptTail {
    private(set) var offset: UInt64 = 0
    private var partial = Data()

    /// The whole lines a piece completes; a piece from another offset (the
    /// file started over, or a piece was lost) starts the reading over.
    mutating func take(_ chunk: RemoteChunk) -> (lines: [Data], restarted: Bool) {
        var restarted = false
        if chunk.offset != offset {
            restarted = true
            partial = Data()
            offset = chunk.offset
        }
        offset += UInt64(chunk.bytes.count)
        partial.append(chunk.bytes)
        var lines: [Data] = []
        while let end = partial.firstIndex(of: 0x0A) {
            let line = partial[partial.startIndex..<end]
            if !line.isEmpty { lines.append(Data(line)) }
            partial = Data(partial[partial.index(after: end)...])
        }
        // A line longer than any Claude Code writes is not waited on forever.
        if partial.count > 8 << 20 { partial = Data() }
        return (lines, restarted)
    }
}

// MARK: Hooks on the host

/// The island's hooks in a host's Claude Code settings. The settings are read
/// over SSH and changed here, with the same rules as on this Mac, then
/// written back in one step; the file as it was before the first setup stays
/// on the host so "Remove from host" can put it back exactly.
enum RemoteHookInstaller {
    static func marker(bundle: String) -> String { "# openbar-remote-hook \(bundle)" }

    static func quoted(_ text: String) -> String { "'" + text.replacingOccurrences(of: "'", with: "'\\''") + "'" }

    /// What each hook runs: the host's own client writing to the forwarded
    /// socket. With nothing listening it fails at once and Claude Code goes on.
    static func command(client: RemoteHookClient, socket: String, bundle: String) -> String {
        let run: String
        switch client {
        case .nc(let path): run = "\(quoted(path)) -U \(quoted(socket))"
        case .socat(let path): run = "\(quoted(path)) - \(quoted("UNIX-CONNECT:" + socket))"
        case .python3(let path):
            let code = "import socket,sys;s=socket.socket(socket.AF_UNIX);s.connect(sys.argv[1]);"
                + "s.sendall(sys.stdin.buffer.read());s.shutdown(socket.SHUT_WR);"
                + "[sys.stdout.buffer.write(c) for c in iter(lambda:s.recv(65536),b\"\")]"
            run = "\(quoted(path)) -c \(quoted(code)) \(quoted(socket))"
        }
        return "\(run) 2>/dev/null; exit 0 \(marker(bundle: bundle))"
    }

    static func installing(into settings: [String: Any], command: String, bundle: String) -> [String: Any] {
        ClaudeHookInstaller.installing(into: settings, command: command, marker: marker(bundle: bundle))
    }

    static func removing(from settings: [String: Any], bundle: String) -> [String: Any] {
        ClaudeHookInstaller.removing(from: settings, marker: marker(bundle: bundle))
    }

    static func hasAny(in settings: [String: Any], bundle: String) -> Bool {
        ClaudeHookInstaller.hasAny(in: settings, marker: marker(bundle: bundle))
    }

    /// The settings as the host keeps them: none, or JSON Claude Code reads.
    static func parse(_ data: Data?) throws -> [String: Any] {
        guard let data, !data.allSatisfy({ $0 == 0x20 || $0 == 0x0A || $0 == 0x0D || $0 == 0x09 }) else { return [:] }
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
            throw ClaudeHookInstaller.Failure.unreadable
        }
        return json
    }

    static func serialize(_ settings: [String: Any]) throws -> Data {
        var data = try JSONSerialization.data(withJSONObject: settings, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])
        data.append(0x0A)
        return data
    }

    /// The copy kept at the first setup: the old file, or the fact there was none.
    enum Backup: Equatable {
        case file(Data)
        case none
    }

    enum Removal: Equatable {
        /// Only the hooks changed since: the kept copy goes back as it was.
        case restore
        /// Something else changed too: the hooks come out and the rest stays.
        case write(Data)
    }

    static func removal(current: Data?, backup: Backup?, bundle: String) throws -> Removal {
        let without = removing(from: try parse(current), bundle: bundle)
        if let backup {
            let before: [String: Any]
            switch backup {
            case .none: before = [:]
            case .file(let data): before = (try? parse(data)) ?? ["openbar": "unreadable"]
            }
            if NSDictionary(dictionary: without).isEqual(to: before) { return .restore }
        }
        return .write(try serialize(without))
    }
}
