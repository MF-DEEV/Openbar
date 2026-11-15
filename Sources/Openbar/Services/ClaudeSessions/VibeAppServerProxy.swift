// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// Mistral Vibe's terminal UI talks to its own `vibe-app-server` over a pipe
/// (JSON-RPC, one message a line). A session started from New session runs
/// that server through this proxy: the lines pass in both directions as they
/// are, and a `callback/call` that asks the client for an approval or an
/// answer is also told to the island, which can answer it with a
/// `callback/result` of its own. The first answer wins, as in Codex: a late
/// one comes back from the server as a conflict and is dropped. With the
/// island not running, or nothing answering, the terminal asks as before.
///
///     Openbar --vibe-proxy <hook socket> <vibe-app-server> [arguments]
enum VibeProxy {
    static let flag = "--vibe-proxy"

    static func isRequested(by arguments: [String]) -> Bool { arguments.dropFirst().first == flag }

    static func runAndExit() -> Never {
        let arguments = Array(CommandLine.arguments.dropFirst(2))
        guard arguments.count >= 2 else {
            FileHandle.standardError.write(Data("usage: \(flag) <socket> <vibe-app-server> [arguments]\n".utf8))
            exit(64)
        }
        exit(VibeProxyProcess(socket: arguments[0], server: arguments[1], arguments: Array(arguments.dropFirst(2))).run())
    }
}

/// What the island is asked about: one approval or question of the session.
struct VibeCallback: Equatable {
    enum Kind: String, Equatable { case approval, question }

    struct Question: Equatable {
        struct Option: Equatable {
            let label: String
            let description: String
        }
        let question: String
        let header: String
        let options: [Option]
        let multiSelect: Bool
    }

    let id: String
    let session: String
    let kind: Kind
    /// Vibe's own name for the tool, as `file_system.bash`.
    let tool: String
    let input: [String: String]
    /// Whether "for this session" is one of the choices the server offers.
    let offersSession: Bool
    let questions: [Question]

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.id == rhs.id && lhs.session == rhs.session && lhs.kind == rhs.kind && lhs.tool == rhs.tool
            && lhs.input == rhs.input && lhs.offersSession == rhs.offersSession && lhs.questions == rhs.questions
    }
}

enum VibeWire {
    // MARK: What the server says

    /// The approval or question a `callback/call` carries; other callbacks
    /// (a connector's sign-in) are the terminal's.
    static func callback(inServerLine line: Data) -> VibeCallback? {
        guard contains(line, "\"callback/call\""),
              let json = object(line), json["method"] as? String == "callback/call",
              let params = json["params"] as? [String: Any], let callback = params["callback"] as? [String: Any],
              let detail = callback["detail"] as? [String: Any],
              let id = callback["callbackId"] as? String ?? callback["id"] as? String, !id.isEmpty,
              let session = callback["sessionId"] as? String, !session.isEmpty else { return nil }
        switch detail["kind"] as? String {
        case "approval":
            let effect = detail["effect"] as? [String: Any] ?? [:]
            let choices = detail["choices"] as? [String] ?? []
            // The server's own order: an approval with no choice to give is the terminal's.
            guard choices.isEmpty || choices.contains("approve") else { return nil }
            return VibeCallback(id: id, session: session, kind: .approval, tool: effect["toolName"] as? String ?? "",
                                input: strings(effect["input"]), offersSession: choices.contains("approve_for_session"),
                                questions: [])
        case "user_input":
            let request = detail["request"] as? [String: Any] ?? [:]
            let questions = (request["questions"] as? [[String: Any]] ?? []).compactMap { item -> VibeCallback.Question? in
                guard let text = item["question"] as? String, !text.isEmpty else { return nil }
                let options = (item["options"] as? [[String: Any]] ?? []).compactMap { option -> VibeCallback.Question.Option? in
                    guard let label = option["label"] as? String, !label.isEmpty else { return nil }
                    return .init(label: label, description: option["description"] as? String ?? "")
                }
                return .init(question: text, header: item["header"] as? String ?? "", options: options,
                             multiSelect: item["multiSelect"] as? Bool ?? false)
            }
            guard !questions.isEmpty else { return nil }
            return VibeCallback(id: id, session: session, kind: .question, tool: "", input: [:], offersSession: false,
                                questions: questions)
        default:
            return nil
        }
    }

    /// The callback a server line says is no longer open: answered, cancelled
    /// or expired, by the terminal or anywhere else.
    static func settledCallback(inServerLine line: Data) -> String? {
        guard contains(line, "\"history/entryUpdated\""), let json = object(line),
              json["method"] as? String == "history/entryUpdated", let params = json["params"] as? [String: Any],
              let entry = params["entryId"] as? String, let patch = params["patch"] as? [[String: Any]] else { return nil }
        for change in patch where change["path"] as? String == "/state" {
            if let state = change["value"] as? [String: Any], let status = state["status"] as? String, status != "open" {
                return entry
            }
        }
        return nil
    }

    /// A session the server speaks of, with its folder when the line has it.
    static func session(inServerLine line: Data) -> (id: String, cwd: String?)? {
        guard contains(line, "\"sessionId\"") || contains(line, "\"session\":"), let json = object(line) else { return nil }
        if let params = json["params"] as? [String: Any], let id = params["sessionId"] as? String, !id.isEmpty {
            return (id, nil)
        }
        if let result = json["result"] as? [String: Any], let state = result["state"] as? [String: Any],
           let session = state["session"] as? [String: Any], let id = session["id"] as? String, !id.isEmpty {
            return (id, session["cwd"] as? String)
        }
        return nil
    }

    // MARK: What the client says

    /// The callback a client line answers.
    static func answeredCallback(inClientLine line: Data) -> String? {
        guard contains(line, "\"callback/result\""), let json = object(line), json["method"] as? String == "callback/result",
              let params = json["params"] as? [String: Any], let result = params["result"] as? [String: Any] else { return nil }
        return result["callbackId"] as? String
    }

    // MARK: What the island answers

    /// The `callback/result` request for the island's reply:
    /// `{"decision": "approve" | "approve_for_session" | "deny", "feedback": "…"}` for an approval,
    /// `{"answers": [{"question", "answer", "isOther"}]}` or `{"cancelled": true}` for questions.
    static func result(id: String, callback: VibeCallback, reply: [String: Any]) -> Data? {
        let output: [String: Any]
        switch callback.kind {
        case .approval:
            guard let decision = reply["decision"] as? String,
                  ["approve", "approve_for_session", "deny"].contains(decision),
                  decision != "approve_for_session" || callback.offersSession else { return nil }
            var body: [String: Any] = ["type": "approval", "decision": ["type": decision]]
            if let feedback = reply["feedback"] as? String, !feedback.isEmpty { body["feedback"] = feedback }
            output = body
        case .question:
            if reply["cancelled"] as? Bool == true {
                output = ["type": "user_input", "result": ["answers": [], "cancelled": true]]
            } else {
                guard let answers = reply["answers"] as? [[String: Any]], !answers.isEmpty else { return nil }
                let known = Set(callback.questions.map(\.question))
                let kept = answers.compactMap { answer -> [String: Any]? in
                    guard let question = answer["question"] as? String, known.contains(question),
                          let text = answer["answer"] as? String else { return nil }
                    return ["question": question, "answer": text, "isOther": answer["isOther"] as? Bool ?? false]
                }
                guard kept.count == answers.count else { return nil }
                output = ["type": "user_input", "result": ["answers": kept, "cancelled": false]]
            }
        }
        let request: [String: Any] = [
            "jsonrpc": "2.0", "id": id, "method": "callback/result",
            "params": ["sessionId": callback.session, "result": ["callbackId": callback.id, "output": output]],
        ]
        return try? JSONSerialization.data(withJSONObject: request, options: [.sortedKeys])
    }

    /// The server's answer to a request, by its id: whether it took it and
    /// how far the event stream had got. The proxy's own requests carry ids
    /// of its own, and their answers are not the terminal's.
    static func response(inServerLine line: Data, idPrefix: String) -> (id: String, accepted: Bool, lastEventId: Int)? {
        guard contains(line, "\"id\":\"\(idPrefix)"), let json = object(line), let id = json["id"] as? String,
              id.hasPrefix(idPrefix) else { return nil }
        let result = json["result"] as? [String: Any]
        return (id, result?["accepted"] as? Bool ?? false, (result?["lastEventId"] as? NSNumber)?.intValue ?? 0)
    }

    /// What the server would answer a terminal that answers a callback the
    /// island already answered: its terminal UI keeps the approval's dialog
    /// open on the server's `conflict` and closes it on a taken answer, so the
    /// proxy says the answer was taken. `line` is the terminal's `callback/result` request.
    static func acknowledgement(forClientLine line: Data, lastEventId: Int) -> Data? {
        guard let json = object(line), let id = json["id"], json["method"] as? String == "callback/result" else { return nil }
        let reply: [String: Any] = ["jsonrpc": "2.0", "id": id, "result": ["lastEventId": lastEventId, "accepted": true]]
        guard var data = try? JSONSerialization.data(withJSONObject: reply, options: [.sortedKeys]) else { return nil }
        data.append(0x0A)
        return data
    }

    // MARK: What the island is told

    /// The envelope for one of the proxy's messages (`hook` is `proxy`).
    static func envelope(_ event: [String: Any], pid: Int32) -> Data? {
        let body: [String: Any] = [AgentHookMessage.key: "vibe", "hook": "proxy", "pid": Int(pid), "event": event]
        return try? JSONSerialization.data(withJSONObject: body, options: [.sortedKeys])
    }

    static func event(for callback: VibeCallback) -> [String: Any] {
        var event: [String: Any] = ["kind": callback.kind.rawValue, "session": callback.session, "request": callback.id]
        switch callback.kind {
        case .approval:
            event["tool"] = callback.tool
            event["input"] = callback.input
            event["always"] = callback.offersSession
        case .question:
            event["questions"] = callback.questions.map { question -> [String: Any] in
                ["question": question.question, "header": question.header, "multiSelect": question.multiSelect,
                 "options": question.options.map { ["label": $0.label, "description": $0.description] }]
            }
        }
        return event
    }

    // MARK: Reading

    private static func contains(_ data: Data, _ needle: String) -> Bool { data.range(of: Data(needle.utf8)) != nil }

    private static func object(_ data: Data) -> [String: Any]? {
        (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    /// The scalar members of an object, as text; what a person can read in a card.
    private static func strings(_ value: Any?) -> [String: String] {
        guard let object = value as? [String: Any] else { return [:] }
        var out: [String: String] = [:]
        for (key, item) in object {
            if let text = item as? String { out[key] = text } else if let number = item as? NSNumber { out[key] = number.stringValue }
        }
        return out
    }
}

/// The proxy process: relays the two pipes and asks the island about callbacks.
final class VibeProxyProcess {
    private let socket: String
    private let server: String
    private let arguments: [String]
    private let child = Process()
    private let toServer = Pipe()
    private let fromServer = Pipe()
    private let writeLock = NSLock()
    private let outLock = NSLock()
    static let idPrefix = "openbar-"
    /// What the island is asked, by callback: the connection waiting for its answer.
    private let state = NSLock()
    private var waiting: [String: (callback: VibeCallback, fd: Int32)] = [:]
    private var told = Set<String>()
    /// The proxy's own `callback/result` requests, by id, and the callbacks the
    /// island has answered, with the event the server had reached.
    private var ours: [String: String] = [:]
    private var answeredByIsland: [String: Int] = [:]
    private var counter = 0
    /// Asking the island never holds up the relay.
    private let island = DispatchQueue(label: "com.openbar.vibe-proxy.island", attributes: .concurrent)

    init(socket: String, server: String, arguments: [String]) {
        self.socket = socket
        self.server = server
        self.arguments = arguments
    }

    func run() -> Int32 {
        child.executableURL = URL(fileURLWithPath: server)
        child.arguments = arguments
        child.standardInput = toServer
        child.standardOutput = fromServer
        child.standardError = FileHandle.standardError
        do { try child.run() } catch {
            FileHandle.standardError.write(Data("vibe proxy: cannot start \(server): \(error.localizedDescription)\n".utf8))
            return 127
        }
        signal(SIGPIPE, SIG_IGN)
        for name in [SIGTERM, SIGINT, SIGHUP] {
            signal(name, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: name, queue: .global())
            source.setEventHandler { [child] in child.terminate() }
            source.resume()
            signalSources.append(source)
        }
        let reader = Thread { [self] in readClient() }
        reader.start()
        readServer()
        child.waitUntilExit()
        return child.terminationStatus
    }

    private var signalSources: [DispatchSourceSignal] = []

    // MARK: Relay

    /// The terminal's lines to the server, until the terminal closes its end.
    private func readClient() {
        let input = FileHandle.standardInput
        var buffer = Data()
        while true {
            let chunk = input.availableData
            if chunk.isEmpty { break }
            buffer.append(chunk)
            while let newline = buffer.firstIndex(of: 0x0A) {
                let line = Data(buffer[buffer.startIndex...newline])
                buffer.removeSubrange(buffer.startIndex...newline)
                if let answered = VibeWire.answeredCallback(inClientLine: line) {
                    // The island answered first: the terminal's answer is taken as it is, not sent.
                    if let reached = state.withLock({ answeredByIsland[answered] }),
                       let taken = VibeWire.acknowledgement(forClientLine: line, lastEventId: reached) {
                        emit(taken)
                        continue
                    }
                    send(line)
                    settle(answered)
                } else {
                    send(line)
                }
            }
        }
        try? toServer.fileHandleForWriting.close()
    }

    /// The server's lines to the terminal, as they come.
    private func readServer() {
        let input = fromServer.fileHandleForReading
        var buffer = Data()
        while true {
            let chunk = input.availableData
            if chunk.isEmpty { break }
            buffer.append(chunk)
            while let newline = buffer.firstIndex(of: 0x0A) {
                let line = Data(buffer[buffer.startIndex...newline])
                buffer.removeSubrange(buffer.startIndex...newline)
                if let own = VibeWire.response(inServerLine: line, idPrefix: Self.idPrefix) {
                    state.withLock {
                        if let callback = ours.removeValue(forKey: own.id), own.accepted { answeredByIsland[callback] = own.lastEventId }
                    }
                    continue
                }
                emit(line)
                inspect(line)
            }
        }
        if !buffer.isEmpty { emit(buffer) }
    }

    /// To the terminal: a line is written whole, whichever thread has it.
    private func emit(_ data: Data) {
        outLock.lock()
        defer { outLock.unlock() }
        try? FileHandle.standardOutput.write(contentsOf: data)
    }

    private func send(_ line: Data) {
        writeLock.lock()
        defer { writeLock.unlock() }
        var data = line
        if data.last != 0x0A { data.append(0x0A) }
        try? toServer.fileHandleForWriting.write(contentsOf: data)
    }

    // MARK: The island

    private func inspect(_ line: Data) {
        if let settled = VibeWire.settledCallback(inServerLine: line) { settle(settled) }
        if let callback = VibeWire.callback(inServerLine: line) {
            island.async { [self] in ask(callback) }
        } else if let session = VibeWire.session(inServerLine: line) {
            let first = state.withLock { told.insert(session.id).inserted }
            if first {
                island.async { [self] in
                    var event: [String: Any] = ["kind": "attach", "session": session.id]
                    if let cwd = session.cwd { event["cwd"] = cwd }
                    notify(event)
                }
            }
        }
    }

    /// Tells the island of a callback and waits for its answer, which goes to
    /// the server as a `callback/result`. Nothing answered: the terminal's.
    private func ask(_ callback: VibeCallback) {
        guard let message = VibeWire.envelope(VibeWire.event(for: callback), pid: getppid()),
              let fd = Self.connect(socket) else { return }
        let first = state.withLock { () -> Bool in
            guard waiting[callback.id] == nil else { return false }
            waiting[callback.id] = (callback, fd)
            return true
        }
        guard first else { close(fd); return }
        guard Self.write(fd, message) else { finish(callback.id); return }
        let reply = Self.readAll(fd)
        let open = state.withLock { waiting.removeValue(forKey: callback.id) != nil }
        close(fd)
        guard open, let reply, let json = (try? JSONSerialization.jsonObject(with: reply)) as? [String: Any] else { return }
        let id = state.withLock { () -> String in
            counter += 1
            ours["\(Self.idPrefix)\(counter)"] = callback.id
            return "\(Self.idPrefix)\(counter)"
        }
        if let request = VibeWire.result(id: id, callback: callback, reply: json) { send(request) }
    }

    /// The callback was answered elsewhere: the island stops waiting and takes the card down.
    private func settle(_ id: String) {
        guard let taken = state.withLock({ waiting.removeValue(forKey: id) }) else { return }
        shutdown(taken.fd, SHUT_RDWR)
        island.async { [self] in
            notify(["kind": "settled", "session": taken.callback.session, "request": id])
        }
    }

    private func finish(_ id: String) {
        if let taken = state.withLock({ waiting.removeValue(forKey: id) }) { close(taken.fd) }
    }

    /// One message the island answers nothing to.
    private func notify(_ event: [String: Any]) {
        guard let message = VibeWire.envelope(event, pid: getppid()), let fd = Self.connect(socket) else { return }
        _ = Self.write(fd, message)
        close(fd)
    }

    // MARK: Socket

    static func connect(_ path: String) -> Int32? {
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8)
        guard bytes.count < MemoryLayout.size(ofValue: address.sun_path) else { close(fd); return nil }
        withUnsafeMutableBytes(of: &address.sun_path) { buffer in
            for (index, byte) in bytes.enumerated() { buffer[index] = byte }
        }
        let result = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard result == 0 else { close(fd); return nil }
        var off: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &off, socklen_t(MemoryLayout<Int32>.size))
        return fd
    }

    static func write(_ fd: Int32, _ data: Data) -> Bool {
        data.withUnsafeBytes { bytes in
            guard var base = bytes.baseAddress else { return true }
            var left = bytes.count
            while left > 0 {
                let written = Darwin.write(fd, base, left)
                if written > 0 { left -= written; base += written; continue }
                if written < 0, errno == EINTR { continue }
                return false
            }
            return true
        }
    }

    /// Everything the island sends before it closes the connection; nil when it sent nothing.
    static func readAll(_ fd: Int32) -> Data? {
        var data = Data()
        var chunk = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = read(fd, &chunk, chunk.count)
            if count > 0 { data.append(chunk, count: count); if data.count > 1 << 20 { return nil }; continue }
            if count < 0, errno == EINTR { continue }
            break
        }
        return data.isEmpty ? nil : data
    }
}
