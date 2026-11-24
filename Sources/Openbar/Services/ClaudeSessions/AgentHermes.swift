// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import Darwin
import Foundation
import SQLite3

/// Hermes Agent (Nous Research, #244). Its shell hooks sit under `hooks:` in
/// the config.yaml of its home, and its gateway, which runs every session
/// whether a terminal, its Ink UI or its desktop app shows it, runs them.
/// So a hook names the gateway, not the session's terminal, and the folder
/// it reports is the gateway's: the session's own folder and title come from
/// Hermes's state.db, read only, and its terminal from the note each
/// terminal client leaves of the session it shows (`terminal-sessions/`).
enum AgentHermes {
    /// The events the island listens to. Hermes asks the person once for
    /// each event and command the first time it runs them, so they are as
    /// few as the island can do with.
    static let events = ["on_session_open", "pre_llm_call", "pre_tool_call", "post_tool_call", "on_session_end",
                         "on_human_input_request", "on_human_input_resolved", "on_session_finalize"]

    /// Where a session runs that the island lists: a terminal, Hermes's own
    /// terminal UI or desktop app. Chats its gateway answers on Telegram,
    /// Discord, Slack and the rest are left out: they have no window to
    /// open and go on long after anyone looks.
    static let localPlatforms: Set<String> = ["cli", "tui", "desktop", "gui", "local", "oneshot", "acp"]

    /// Hermes's home: `HERMES_HOME`, or `~/.hermes`.
    static func home(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["HERMES_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home.appending(path: ".hermes")
    }

    static func config(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        self.home(home: home, environment: environment).appending(path: "config.yaml")
    }

    /// The hook for one event, as Hermes runs it: split like a shell line
    /// and started with no shell, so the island's usual line goes to `sh`.
    static func command(event: String, socket: String, bundle: String) -> String {
        let line = AgentHookInstaller.command(.hermes, event: event, socket: socket, bundle: bundle)
        return "/bin/sh -c " + shellQuoted(line)
    }

    static func shellQuoted(_ text: String) -> String {
        "'" + text.replacingOccurrences(of: "'", with: "'\"'\"'") + "'"
    }

    // MARK: Approval

    /// Which of the island's hooks Hermes was told to run: the person
    /// answers its prompt once per event in a terminal, or starts it once
    /// with `--accept-hooks`. Read from its allowlist, never written.
    enum Approval: Equatable {
        case approved
        /// Hooks Hermes hasn't been told to run yet: it skips them.
        case waiting(count: Int)
    }

    static func approval(home: URL, socket: String, bundle: String,
                         environment: [String: String] = ProcessInfo.processInfo.environment) -> Approval {
        let file = self.home(home: home, environment: environment).appending(path: "shell-hooks-allowlist.json")
        let approved = approvedPairs(FileManager.default.contents(atPath: file.path))
        let missing = events.filter { !approved.contains(Pair(event: $0, command: command(event: $0, socket: socket, bundle: bundle))) }
        return missing.isEmpty ? .approved : .waiting(count: missing.count)
    }

    struct Pair: Hashable {
        let event: String
        let command: String
    }

    /// The pairs Hermes's allowlist approves (agent/shell_hooks.py: an
    /// `approvals` list of `{event, command}`).
    static func approvedPairs(_ data: Data?) -> Set<Pair> {
        guard let data, data.count < 4 << 20,
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let list = json["approvals"] as? [[String: Any]] else { return [] }
        return Set(list.compactMap { entry in
            guard let event = entry["event"] as? String, let command = entry["command"] as? String else { return nil }
            return Pair(event: event, command: command.trimmingCharacters(in: .whitespacesAndNewlines))
        })
    }
}

// MARK: - config.yaml

/// Hermes's config.yaml, changed as text so the person's comments, order
/// and style stay as they were. Hermes reads it with a parser that refuses
/// a key twice, so the island's entries join the person's `hooks:` mapping
/// when there is one: each is one line of its own under its event, carrying
/// the island's marker in its command, and a key the island adds carries
/// the marker as a comment. Remove takes those lines, and a key left with
/// nothing that the island added or emptied.
enum AgentHermesConfig {
    enum Failure: Error, Equatable {
        /// The hooks are written in a form the island can't add to safely
        /// (a flow mapping, or a flow list with entries): left alone.
        case unsupported
    }

    /// One line of the island's under an event: a flow mapping, so it never
    /// spans lines, with the command in a JSON string, which YAML reads.
    static func item(command: String, timeout: Int) -> String {
        let data = (try? JSONSerialization.data(withJSONObject: [command], options: [.withoutEscapingSlashes])) ?? Data()
        let quoted = String(decoding: data, as: UTF8.self).dropFirst().dropLast()
        return "- {command: \(quoted), timeout: \(timeout)}"
    }

    /// The commands under each event, as Hermes would read them from lines
    /// the island can parse; nil when there is no `hooks:` block.
    static func commands(_ text: String) -> [String: [String]]? {
        let lines = split(text)
        guard let block = hooksBlock(lines) else { return nil }
        var found: [String: [String]] = [:]
        for event in events(lines, block) {
            found[event.name] = event.items.compactMap { command(in: lines[$0].joined(separator: "\n")) }
        }
        return found
    }

    /// The text with the island's lines taken out; nil when it has none.
    static func removing(_ text: String, marker: String) -> String? {
        var lines = split(text)
        guard let block = hooksBlock(lines) else { return nil }
        var drop = IndexSet()
        for event in events(lines, block) {
            let own = event.items.filter { lines[$0].joined(separator: "\n").contains(marker) }
            own.forEach { drop.insert(integersIn: $0) }
            let left = event.items.count - own.count
            if left == 0, !own.isEmpty || lines[event.key].contains(marker) { drop.insert(event.key) }
        }
        let hooksMarked = lines[block.line].contains(marker)
        guard !drop.isEmpty || hooksMarked else { return nil }
        // `hooks:` goes when nothing is left under it and the island added it
        // or emptied it.
        let children = (block.line + 1..<block.end).filter { index in
            !drop.contains(index) && !isBlankOrComment(lines[index])
        }
        if children.isEmpty, hooksMarked || !drop.isEmpty {
            drop.insert(block.line)
            for index in block.line + 1..<block.end where !drop.contains(index) && lines[index].trimmingCharacters(in: .whitespaces).isEmpty {
                drop.insert(index)
            }
        }
        for index in drop.reversed() { lines.remove(at: index) }
        while let last = lines.last, last.trimmingCharacters(in: .whitespaces).isEmpty { lines.removeLast() }
        return lines.isEmpty ? "" : lines.joined(separator: "\n") + "\n"
    }

    /// The text with the island's line under each event, after the person's
    /// own, and any older lines of the island's taken out first.
    static func installing(_ text: String?, entries: [(event: String, command: String)], timeout: Int,
                           marker: String) throws -> String {
        var base = text.map { removing($0, marker: marker) ?? $0 } ?? ""
        var lines = split(base)
        guard var block = hooksBlock(lines) else {
            // No hooks yet: the island's block at the end.
            if !base.isEmpty, !base.hasSuffix("\n") { base += "\n" }
            if !base.isEmpty { base += "\n" }
            var block = ["hooks:  \(marker)"]
            for entry in entries {
                block.append("  \(entry.event):  \(marker)")
                block.append("    " + item(command: entry.command, timeout: timeout))
            }
            return base + block.joined(separator: "\n") + "\n"
        }
        guard block.form != .unsupported else { throw Failure.unsupported }
        if block.form == .empty {
            // `hooks: {}` or `hooks: null` becomes the island's block.
            lines[block.line] = "hooks:  \(marker)"
            block = hooksBlock(lines) ?? block
        }
        let parsed = events(lines, block)
        let indent = parsed.first.map { lines[$0.key].prefix { $0 == " " }.count } ?? 2
        var appended: [String] = []
        // Lines go in from the bottom up so earlier indexes hold.
        var insertions: [(at: Int, lines: [String])] = []
        for entry in entries {
            let line = item(command: entry.command, timeout: timeout)
            if let event = parsed.first(where: { $0.name == entry.event }) {
                switch event.form {
                case .unsupported: throw Failure.unsupported
                case .empty:
                    // `event: []` or `event: null` takes the island's line under it.
                    lines[event.key] = String(repeating: " ", count: indent) + "\(entry.event):  \(marker)"
                    insertions.append((event.key + 1, [String(repeating: " ", count: indent + 2) + line]))
                case .block:
                    let itemIndent = event.items.first.map { lines[$0.lowerBound].prefix { $0 == " " }.count } ?? indent + 2
                    let after = event.items.last.map { $0.upperBound } ?? event.key + 1
                    insertions.append((after, [String(repeating: " ", count: itemIndent) + line]))
                }
            } else {
                appended.append(String(repeating: " ", count: indent) + "\(entry.event):  \(marker)")
                appended.append(String(repeating: " ", count: indent + 2) + line)
            }
        }
        // New keys go in first where an entry goes at the same line, so the
        // entry ends up under its own key, above them.
        if !appended.isEmpty { insertions.insert((block.content, appended), at: 0) }
        let ordered = insertions.enumerated().sorted { $0.element.at != $1.element.at ? $0.element.at > $1.element.at : $0.offset < $1.offset }
        for insertion in ordered.map(\.element) {
            lines.insert(contentsOf: insertion.lines, at: insertion.at)
        }
        var result = lines.joined(separator: "\n")
        if !result.hasSuffix("\n") { result += "\n" }
        return result
    }

    // MARK: Parsing

    enum Form: Equatable {
        /// Nothing on the key's line: its entries are on the lines under it.
        case block
        /// `{}`, `[]`, `null` or `~`: nothing in it.
        case empty
        /// Entries on the key's own line, in a flow mapping or list.
        case unsupported
    }

    struct Block {
        /// The `hooks:` line.
        let line: Int
        /// One past its last line: the next line at the left edge that is
        /// not a comment, or the end.
        let end: Int
        /// One past its last line with content: where new keys go.
        let content: Int
        let form: Form
    }

    struct Event {
        let name: String
        let key: Int
        let end: Int
        let form: Form
        /// Each entry's lines.
        let items: [Range<Int>]
    }

    static func split(_ text: String) -> [String] {
        var lines = text.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
        if lines.last == "" { lines.removeLast() }
        return lines
    }

    static func indent(_ line: String) -> Int { line.prefix { $0 == " " }.count }

    static func isBlankOrComment(_ line: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty || trimmed.hasPrefix("#")
    }

    /// What follows a key's colon, without a comment after it.
    static func form(_ value: Substring) -> Form {
        var value = String(value)
        if let hash = value.range(of: " #") ?? (value.hasPrefix("#") ? value.startIndex..<value.index(after: value.startIndex) : nil) {
            value = String(value[..<hash.lowerBound])
        }
        let trimmed = value.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return .block }
        if ["{}", "[]", "null", "~", "Null", "NULL"].contains(trimmed) { return .empty }
        return .unsupported
    }

    static func hooksBlock(_ lines: [String]) -> Block? {
        guard let line = lines.firstIndex(where: { $0.hasPrefix("hooks:") }) else { return nil }
        let form = form(lines[line].dropFirst("hooks:".count))
        var end = line + 1
        while end < lines.count {
            let text = lines[end]
            if !isBlankOrComment(text), indent(text) == 0 { break }
            end += 1
        }
        // New keys go after the last line inside the block: blank lines and
        // comments at the left edge after it belong to what follows.
        var content = end
        while content > line + 1 {
            let text = lines[content - 1]
            guard text.trimmingCharacters(in: .whitespaces).isEmpty || (isBlankOrComment(text) && indent(text) == 0) else { break }
            content -= 1
        }
        return Block(line: line, end: end, content: content, form: form)
    }

    static func events(_ lines: [String], _ block: Block) -> [Event] {
        guard block.form == .block else { return [] }
        let body = Array(block.line + 1..<block.end)
        guard let first = body.first(where: { !isBlankOrComment(lines[$0]) }) else { return [] }
        let keyIndent = indent(lines[first])
        var keys: [(name: String, line: Int, form: Form)] = []
        for index in body {
            let text = lines[index]
            guard !isBlankOrComment(text), indent(text) == keyIndent else { continue }
            let rest = text.dropFirst(keyIndent)
            guard !rest.hasPrefix("- "), let colon = rest.firstIndex(of: ":") else { continue }
            let name = String(rest[..<colon]).trimmingCharacters(in: .whitespaces)
            guard !name.isEmpty, name.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "_" }) else { continue }
            keys.append((name, index, form(rest[rest.index(after: colon)...])))
        }
        var events: [Event] = []
        for (position, key) in keys.enumerated() {
            let end = position + 1 < keys.count ? keys[position + 1].line : block.end
            var items: [Range<Int>] = []
            var itemIndent: Int?
            var start: Int?
            var last = key.line
            for index in key.line + 1..<end {
                let text = lines[index]
                if isBlankOrComment(text) { continue }
                let level = indent(text)
                let isItem = text.dropFirst(level).hasPrefix("- ") || text.dropFirst(level) == "-"
                if isItem, itemIndent == nil || level == itemIndent {
                    if let start { items.append(start..<last + 1) }
                    itemIndent = level
                    start = index
                }
                last = index
            }
            if let start { items.append(start..<last + 1) }
            events.append(Event(name: key.name, key: key.line, end: end, form: key.form, items: items))
        }
        return events
    }

    /// The command of an entry the island wrote (a flow mapping on one
    /// line) or the person wrote in block style.
    static func command(in item: String) -> String? {
        guard let range = item.range(of: "command:") else { return nil }
        var value = item[range.upperBound...].trimmingCharacters(in: .whitespaces)
        if value.hasPrefix("\"") {
            // A JSON-like double-quoted string, up to its closing quote.
            var escaped = false
            var end: String.Index?
            for index in value.indices.dropFirst() {
                let character = value[index]
                if escaped { escaped = false; continue }
                if character == "\\" { escaped = true; continue }
                if character == "\"" { end = index; break }
            }
            guard let end else { return nil }
            let literal = String(value[...end])
            return (try? JSONSerialization.jsonObject(with: Data("[\(literal)]".utf8)) as? [String])?.first
        }
        if value.hasPrefix("'") {
            value.removeFirst()
            var out = ""
            var iterator = value.makeIterator()
            while let character = iterator.next() {
                if character == "'" {
                    // '' is a quote inside a single-quoted string.
                    var peek = iterator
                    if peek.next() == "'" { out.append("'"); iterator = peek; continue }
                    return out
                }
                out.append(character)
            }
            return nil
        }
        let plain = value.split(separator: "\n").first.map(String.init) ?? value
        return plain.components(separatedBy: " #").first?.trimmingCharacters(in: CharacterSet(charactersIn: " ,}"))
    }
}

// MARK: - Where a session runs

/// What Hermes's state.db says of a session. Read only, through SQLite's
/// read-only mode, so Hermes's own writes are never in the way.
struct AgentHermesRecord: Equatable {
    var source: String?
    var createdSource: String?
    var cwd: String?
    var title: String?
    var model: String?
    var ended = false

    /// A session of a terminal, Hermes's terminal UI or its app; a chat its
    /// gateway answers is not.
    var isLocal: Bool {
        [source, createdSource].contains { $0.map(AgentHermes.localPlatforms.contains) ?? false }
    }

    /// Where the person sees it: `cli`, `tui` or `desktop`.
    var client: String? {
        switch createdSource ?? source {
        case "gui", "desktop": return "desktop"
        case let other: return other
        }
    }
}

enum AgentHermesStore {
    static func database(_ hermes: URL) -> URL { hermes.appending(path: "state.db") }

    static func record(_ session: String, hermes: URL) -> AgentHermesRecord? {
        query(hermes, "SELECT source, created_source, cwd, title, model, ended_at FROM sessions WHERE id = ?", session) { stmt in
            AgentHermesRecord(source: text(stmt, 0), createdSource: text(stmt, 1), cwd: text(stmt, 2), title: text(stmt, 3),
                              model: text(stmt, 4), ended: sqlite3_column_type(stmt, 5) != SQLITE_NULL)
        }
    }

    /// The last thing the agent wrote in the session, for previews.
    static func lastReply(_ session: String, hermes: URL) -> String? {
        query(hermes, """
            SELECT content FROM messages WHERE session_id = ? AND role = 'assistant' AND content IS NOT NULL
            AND content != '' ORDER BY id DESC LIMIT 1
            """, session) { text($0, 0) }.flatMap { AgentHookMessage.preview($0) }
    }

    private static func query<T>(_ hermes: URL, _ sql: String, _ session: String, _ row: (OpaquePointer) -> T?) -> T? {
        let path = database(hermes).path
        guard FileManager.default.fileExists(atPath: path) else { return nil }
        var db: OpaquePointer?
        defer { sqlite3_close(db) }
        guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX, nil) == SQLITE_OK, let db else { return nil }
        sqlite3_busy_timeout(db, 500)
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK, let stmt else { return nil }
        defer { sqlite3_finalize(stmt) }
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        sqlite3_bind_text(stmt, 1, session, -1, transient)
        guard sqlite3_step(stmt) == SQLITE_ROW else { return nil }
        return row(stmt)
    }

    private static func text(_ stmt: OpaquePointer, _ column: Int32) -> String? {
        guard let raw = sqlite3_column_text(stmt, column) else { return nil }
        let value = String(cString: raw)
        return value.isEmpty ? nil : value
    }
}

/// The process that shows a Hermes session: the terminal client the person
/// typed `hermes` into, or Hermes's app.
enum AgentHermesClients {
    /// Hermes's app (Nous Research's Electron app, every variant of it).
    static let appPrefix = "com.nousresearch.hermes"

    /// The terminals each session was last shown in, newest first, from the
    /// notes Hermes's terminal client leaves for `hermes -c`
    /// (hermes_cli/terminal_breadcrumbs.py): a file per terminal, named
    /// after its device (`tty-dev-ttys004`), holding the session's id.
    static func notes(hermes: URL) -> [String: [String]] {
        let folder = hermes.appending(path: "terminal-sessions")
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: folder.path) else { return [:] }
        var found: [String: [(tty: String, at: Double)]] = [:]
        for name in names.prefix(512) {
            guard let tty = device(name),
                  let data = FileManager.default.contents(atPath: folder.appending(path: name).path), data.count < 16 << 10,
                  let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
                  let session = json["session_id"] as? String, !session.isEmpty else { continue }
            found[session, default: []].append((tty, (json["ts"] as? NSNumber)?.doubleValue ?? 0))
        }
        return found.mapValues { $0.sorted { $0.at > $1.at }.map(\.tty) }
    }

    /// The device a note's name stands for: `tty-dev-ttys004` is
    /// /dev/ttys004. Notes named after a multiplexer's pane say nothing of
    /// the device.
    static func device(_ name: String) -> String? {
        guard name.hasPrefix("tty-dev-") else { return nil }
        let rest = name.dropFirst("tty-dev-".count)
        guard !rest.isEmpty, rest.allSatisfy({ $0.isLetter || $0.isNumber }) else { return nil }
        return "/dev/" + rest
    }

    /// Whether a command line is Hermes's terminal client: `hermes` or
    /// `hermes_cli.main` run by Python, never its gateway.
    static func isClient(_ arguments: [String]) -> Bool {
        guard !arguments.contains("gateway"), !arguments.contains("acp"), !arguments.contains("mcp-serve") else { return false }
        return arguments.prefix(4).contains { argument in
            if argument == "hermes_cli.main" { return true }
            let name = (argument as NSString).lastPathComponent
            // `hermes`, `hermes-agent`, and the light and canary builds' own names.
            return name == "hermes" || (name.hasPrefix("hermes-") && name != "hermes-acp")
        }
    }

    struct Process: Equatable {
        let pid: Int32
        let parent: Int32
        let tty: String?
        let cwd: String?
    }

    /// Hermes's terminal clients running now.
    static func running() -> [Process] {
        let count = proc_listallpids(nil, 0)
        guard count > 0 else { return [] }
        var pids = [Int32](repeating: 0, count: Int(count) + 32)
        let filled = proc_listallpids(&pids, Int32(pids.count * MemoryLayout<Int32>.size))
        guard filled > 0 else { return [] }
        return pids.prefix(Int(filled)).compactMap { pid in
            guard pid > 1, let tty = ClaudeSessionHosts.tty(of: pid), isClient(ClaudeZellij.arguments(of: pid)) else { return nil }
            return Process(pid: pid, parent: AgentHookProcess.parent(of: pid) ?? 0, tty: tty, cwd: ClaudeSessionHosts.cwd(of: pid))
        }
    }

    /// The client showing `session`: Hermes's own client on the terminal its
    /// note names; with no note (its terminal UI writes none, and a new
    /// session's note is written only after its first hook), the only
    /// client in the session's folder no other listed session claims. A
    /// note of a session no longer listed says nothing: terminals are used
    /// again. The first of a chain of Hermes processes on a terminal is the
    /// one the person started.
    static func client(of session: String, cwd: String?, notes: [String: [String]], running: [Process],
                       claimed: Set<Int32> = [], listed: Set<String> = []) -> Int32? {
        let first = { (candidates: [Process]) -> Process? in
            let pids = Set(candidates.map(\.pid))
            return candidates.first { !pids.contains($0.parent) } ?? candidates.first
        }
        for tty in notes[session] ?? [] {
            if let found = first(running.filter { $0.tty == tty }) { return found.pid }
        }
        guard let cwd, !cwd.isEmpty else { return nil }
        let standard = { (path: String) in URL(fileURLWithPath: path).standardizedFileURL.resolvingSymlinksInPath().path }
        let wanted = standard(cwd)
        // Terminals another listed session's note names are that session's.
        let noted = Set(notes.filter { $0.key != session && listed.contains($0.key) }.values.flatMap { $0 })
        let here = running.filter { process in
            !claimed.contains(process.pid) && process.cwd.map(standard) == wanted && !(process.tty.map(noted.contains) ?? false)
        }
        let roots = here.filter { process in !here.contains { $0.pid == process.parent } }
        return roots.count == 1 ? roots[0].pid : nil
    }

    /// Hermes's app, when it runs.
    static func app() -> Int32? {
        NSWorkspace.shared.runningApplications.first { $0.bundleIdentifier?.hasPrefix(appPrefix) == true }?.processIdentifier
    }
}

/// Where each Hermes session runs, looked up as its hooks come in and kept
/// while its client lives. Not thread-safe: the service uses it on its queue.
final class AgentHermesPlaces {
    struct Place: Equatable {
        /// The client showing it, or 0 when none was found.
        var pid: Int32
        var record: AgentHermesRecord?
        var looked: Date
    }

    /// How often a session whose client wasn't found is looked for again:
    /// its note comes a moment after its first hooks.
    static let retryAfter: TimeInterval = 0.5

    private var places: [String: Place] = [:]
    private let alive: (Int32) -> Bool

    init(alive: @escaping (Int32) -> Bool = AgentHookSessions.isAlive) { self.alive = alive }

    /// The session's place; nil for a chat its gateway answers, which the
    /// island doesn't list. `fresh` reads its record again (a title comes
    /// after the first turn).
    func place(_ session: String, hermes: URL, now: Date, fresh: Bool = false,
               record: (String) -> AgentHermesRecord? = { _ in nil },
               notes: () -> [String: [String]] = { [:] }, running: () -> [AgentHermesClients.Process] = { [] },
               app: () -> Int32? = { nil }) -> Place? {
        var place = places[session]
        if place == nil || fresh || place?.record == nil { 
            let read = record(session)
            if place == nil { place = Place(pid: 0, record: read, looked: .distantPast) }
            if let read { place?.record = read }
        }
        guard var place else { return nil }
        if let record = place.record, !record.isLocal {
            places[session] = place
            return nil
        }
        let lost = place.pid <= 0 || !alive(place.pid)
        if lost, now.timeIntervalSince(place.looked) >= Self.retryAfter {
            let others = places.filter { $0.key != session && $0.value.record?.isLocal != false }
            place.pid = AgentHermesClients.client(of: session, cwd: place.record?.cwd, notes: notes(), running: running(),
                                                  claimed: Set(others.map(\.value.pid)), listed: Set(others.keys))
                ?? (place.record?.client == "desktop" ? app() : nil) ?? 0
            place.looked = now
        } else if lost {
            place.pid = 0
        }
        places[session] = place
        return place
    }

    func forget(_ session: String) { places[session] = nil }

    /// Forgets the sessions the island no longer lists.
    func keep(_ sessions: Set<String>) { places = places.filter { sessions.contains($0.key) } }
}

/// The keys that answer an agent's prompt in its terminal, for the agents
/// whose hooks can't answer it.
enum AgentHookKeys {
    /// Grok's prompt: Return for the yes it has picked, Ctrl-C for no.
    static func grok(_ decision: ClaudeHookMessage.Decision) -> [ClaudeTypingStep]? {
        switch decision {
        case .allow: return [.enter]
        case .deny: return [.interrupt]
        default: return nil
        }
    }

    /// Hermes's classic prompt (hermes_cli/cli_modal_mixin.py): Allow once
    /// is picked first and Return confirms it; Deny is the last choice but
    /// View, which shows a long command whole, and the arrows stop at the
    /// ends, so three downs reach it whichever choices are offered. A
    /// question with choices takes its number, which answers it at once.
    static func hermes(_ decision: ClaudeHookMessage.Decision, _ asked: ClaudePermission) -> [ClaudeTypingStep]? {
        switch decision {
        case .allow where !asked.asksQuestions: return [.enter]
        case .deny where !asked.asksQuestions: return [.keys([.down, .down, .down]), .enter]
        default: return nil
        }
    }

}

/// Answers a prompt through Hermes's own gateway, the way its Ink UI and its
/// app do (#244): `hermes gateway attach --json` hands out a one-use ticket
/// for the gateway's local WebSocket, the island joins the session as one
/// more viewer (`session.resume`, which lists the prompts waiting), answers
/// (`approval.respond`, `clarify.lock`) and leaves. Hermes's Ink UI and
/// app close their own card when the answer comes from elsewhere, and so
/// does its classic terminal UI for a question; it keeps an approval's menu
/// up until a key is pressed, so there the island presses keys instead
/// whenever the terminal takes them.
enum AgentHermesGateway {
    enum Answer: Equatable {
        /// `once` or `deny` for a dangerous command.
        case approval(String)
        /// Each question's answer: its picks, or nil when skipped.
        case clarify([[String]?])

        var kind: String {
            switch self {
            case .approval: return "approval"
            case .clarify: return "clarify"
            }
        }
    }

    /// What answers the island's decision, or nil for one Hermes can't take.
    static func answer(_ decision: ClaudeHookMessage.Decision, _ asked: ClaudePermission) -> Answer? {
        switch decision {
        case .allow where !asked.asksQuestions: return .approval("once")
        case .deny where !asked.asksQuestions: return .approval("deny")
        case .deny where !asked.questions.isEmpty: return .clarify(asked.questions.map { _ in nil })
        case .answer(let answers) where !asked.questions.isEmpty:
            let picks: [[String]?] = asked.questions.map { question in
                let picked = (answers[question.question] ?? (asked.questions.count == 1 && answers.count == 1 ? answers.values.first : nil))?
                    .filter { !$0.isEmpty } ?? []
                return picked.isEmpty ? nil : picked
            }
            return picks.allSatisfy { $0 == nil } ? nil : .clarify(picks)
        default: return nil
        }
    }

    /// The ticket `hermes gateway attach --json` prints: the socket's URL and
    /// the subprotocols that carry the ticket.
    static func ticket(_ output: String) -> (url: URL, protocols: [String])? {
        guard let data = output.data(using: .utf8),
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              json["state"] as? String == "ready" || json["state"] == nil,
              let text = json["url"] as? String, let url = URL(string: text),
              url.scheme == "ws", let host = url.host, ["127.0.0.1", "localhost", "::1"].contains(host),
              let protocols = json["protocols"] as? [String], !protocols.isEmpty else { return nil }
        return (url, protocols)
    }

    /// The prompt waiting in a session, from `session.resume`'s result: its
    /// id and the run it belongs to.
    static func pending(_ result: [String: Any], kind: String) -> (prompt: String, generation: Int)? {
        let prompts = result["prompts"] as? [[String: Any]] ?? []
        guard let prompt = prompts.last(where: { $0["kind"] as? String == kind }),
              let id = prompt["prompt_id"] as? String, !id.isEmpty else { return nil }
        let generation = (prompt["execution_generation"] as? NSNumber ?? result["execution_generation"] as? NSNumber)?.intValue ?? 0
        return (id, generation)
    }

    /// A question Hermes's gateway asks its Ink UI and app as a request of
    /// its own (`open_requests` in `session.resume`'s result): its id and
    /// each question's id, answered one by one with `clarify.lock`.
    static func openQuestion(_ result: [String: Any]) -> (request: String, questions: [String])? {
        let open = result["open_requests"] as? [[String: Any]] ?? []
        guard let request = open.last(where: { $0["method"] as? String == "clarify" }),
              let id = request["id"] as? String, !id.isEmpty,
              let questions = (request["params"] as? [String: Any])?["questions"] as? [[String: Any]] else { return nil }
        let ids = questions.compactMap { $0["qid"] as? String }
        return ids.isEmpty ? nil : (id, ids)
    }

    /// Answers the prompt waiting in `session`. Blocks a few seconds at most;
    /// call it off the main thread.
    static func send(_ answer: Answer, session: String, hermes executable: String, environment: [String: String],
                     timeout: TimeInterval = 8) -> Bool {
        guard let printed = ClaudeHostCommand.run(executable, ["gateway", "attach", "--json"], environment: environment,
                                                  timeout: timeout),
              let ticket = ticket(printed) else { return false }
        let connection = Connection(url: ticket.url, protocols: ticket.protocols, timeout: timeout)
        defer { connection.close() }
        guard let resumed = connection.call("session.resume", ["session_id": session]) else { return false }
        // A question is the gateway's own request: each answer locked in turn, the last lock
        // settling it; a skipped one is locked empty, several picks as a list.
        if case .clarify(let picks) = answer, let open = openQuestion(resumed) {
            for (index, question) in open.questions.enumerated() {
                let picked = index < picks.count ? picks[index] : nil
                let value: Any = picked.map { $0.count == 1 ? $0[0] as Any : $0 as Any } ?? NSNull()
                guard let locked = connection.call("clarify.lock", ["request_id": open.request, "question_id": question,
                                                                    "answer": value]),
                      locked["status"] as? String != "expired" else { return false }
            }
            return true
        }
        guard let waiting = pending(resumed, kind: answer.kind) else { return false }
        var params: [String: Any] = ["session_id": session, "execution_generation": waiting.generation,
                                     "prompt_id": waiting.prompt]
        let method: String
        switch answer {
        case .approval(let choice):
            method = "approval.respond"
            params["choice"] = choice
        case .clarify(let picks):
            method = "clarify.respond"
            params["answer"] = picks.first.flatMap { $0 }?.joined(separator: ", ") ?? ""
        }
        guard let result = connection.call(method, params) else { return false }
        return ["resolved", "already_resolved"].contains(result["status"] as? String ?? "")
    }

    /// One JSON-RPC conversation over the gateway's WebSocket.
    final class Connection {
        private let task: URLSessionWebSocketTask
        private let session: URLSession
        private let timeout: TimeInterval
        private var next = 0

        init(url: URL, protocols: [String], timeout: TimeInterval) {
            let configuration = URLSessionConfiguration.ephemeral
            configuration.timeoutIntervalForRequest = timeout
            configuration.connectionProxyDictionary = [:]
            session = URLSession(configuration: configuration)
            task = session.webSocketTask(with: url, protocols: protocols)
            self.timeout = timeout
            task.resume()
        }

        /// The call's result, skipping the events that come between; nil on
        /// an error, a refusal or no answer in time.
        func call(_ method: String, _ params: [String: Any]) -> [String: Any]? {
            next += 1
            let id = next
            let request: [String: Any] = ["jsonrpc": "2.0", "id": id, "method": method, "params": params]
            guard let data = try? JSONSerialization.data(withJSONObject: request),
                  let text = String(data: data, encoding: .utf8) else { return nil }
            let sent = DispatchSemaphore(value: 0)
            var failed = false
            task.send(.string(text)) { error in
                failed = error != nil
                sent.signal()
            }
            guard sent.wait(timeout: .now() + timeout) == .success, !failed else { return nil }
            let deadline = Date().addingTimeInterval(timeout)
            while Date() < deadline {
                guard let message = receive(until: deadline),
                      let json = (try? JSONSerialization.jsonObject(with: message)) as? [String: Any] else { return nil }
                guard (json["id"] as? NSNumber)?.intValue == id else { continue }
                return json["result"] as? [String: Any]
            }
            return nil
        }

        private func receive(until deadline: Date) -> Data? {
            let got = DispatchSemaphore(value: 0)
            var data: Data?
            task.receive { result in
                if case .success(let message) = result {
                    switch message {
                    case .string(let text): data = Data(text.utf8)
                    case .data(let bytes): data = bytes
                    @unknown default: break
                    }
                }
                got.signal()
            }
            let left = max(0.1, deadline.timeIntervalSinceNow)
            guard got.wait(timeout: .now() + left) == .success else {
                task.cancel(with: .goingAway, reason: nil)
                return nil
            }
            return data
        }

        func close() {
            task.cancel(with: .normalClosure, reason: nil)
            session.invalidateAndCancel()
        }
    }
}
