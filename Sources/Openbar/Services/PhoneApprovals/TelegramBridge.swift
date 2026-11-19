// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import Combine
import CoreGraphics
import os
import SystemConfiguration

/// Phone approvals through a Telegram bot (#214). While the person is away
/// from the Mac, each request a session waits on goes to their phone with the
/// island's buttons; a tap or a typed reply comes back into the session the
/// way the island's does, marked as coming from the phone. Only the one
/// Telegram account linked in Settings, in its private chat with the bot, is
/// listened to. The bot's token is kept in the keychain.
@MainActor
final class TelegramBridge: ObservableObject {
    static let shared = TelegramBridge()
    static let tokenName = "telegram-bot"
    private static let log = Logger(subsystem: "com.openbar.app", category: "phone-approvals")

    struct Linking: Equatable {
        let code: String
        let expires: Date
        var tries = 0
    }

    /// The bot's @name, once its token checked out.
    @Published private(set) var bot: String?
    @Published private(set) var link: PhoneLink?
    @Published private(set) var linking: Linking?
    /// What went wrong last, for Settings.
    @Published private(set) var problem: String?
    @Published private(set) var checking = false

    /// A request's message on the phone.
    private struct Sent {
        /// Tells this message from a later one for the same request.
        let serial: Int
        let session: String
        let request: String
        let revision: String?
        var message: Int64?
        var text: String
        var picked: [Int: Set<Int>] = [:]
        var typed: [Int: String] = [:]
        var numbers: [Int] = []
        /// A tap went to the session and its answer isn't back yet.
        var answering = false
        /// The message already tells how the request ended.
        var settled = false
    }

    private var token: String?
    private var api: TelegramAPI?
    private var running = false
    private var settings = PhoneSettings.load()
    private var poll: Task<Void, Never>?
    private var pollGeneration = 0
    private var serial = 0
    /// Edits owed to messages still on their way, by the entry's serial.
    private var owed: [Int: String] = [:]
    private var offset: Int64 = 0
    private let run = PhoneCallback.newRun()
    private var startedAt = Date()
    private var subscriptions: Set<AnyCancellable> = []
    private var timer: Timer?
    private var resync: DispatchWorkItem?
    private var sent: [String: Sent] = [:]
    private var actions: [Int: (key: String, action: PhoneAction)] = [:]
    private var nextNumber = 0
    /// Which session each message tells of, so a reply to it goes there.
    private var messageSessions: [Int64: String] = [:]
    private var messageOrder: [Int64] = []
    private var ends: Set<String> = []
    private var endsReady = false
    private var outbox: [() async -> Void] = []
    private var pumping = false

    /// The person can be reached: switched on, a token, a linked chat.
    private var active: Bool { running && settings.enabled && token != nil && link != nil }
    /// Updates are read while linked, and while a code waits to be sent.
    private var listens: Bool { running && token != nil && (active || linking != nil) }

    // MARK: Lifecycle

    /// Starts with the Sessions section.
    func start() {
        guard !running else { return }
        running = true
        startedAt = Date()
        settings = PhoneSettings.load()
        link = PhoneLink.load()
        bot = UserDefaults.standard.string(forKey: DefaultsKey.notchSessionsPhoneBot)
        if token == nil, link != nil || settings.enabled { token = KeychainSecret.read(Self.tokenName) }
        if let token { api = TelegramAPI(token: token) }
        ClaudeSessionService.shared.$sessions
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in MainActor.assumeIsolated { self?.sync() } }
            .store(in: &subscriptions)
        NotificationCenter.default.publisher(for: UserDefaults.didChangeNotification)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in MainActor.assumeIsolated { self?.settingsChanged() } }
            .store(in: &subscriptions)
        for name in ["com.apple.screenIsLocked", "com.apple.screenIsUnlocked"] {
            DistributedNotificationCenter.default().publisher(for: Notification.Name(name))
                .receive(on: DispatchQueue.main)
                .sink { [weak self] _ in MainActor.assumeIsolated { self?.sync() } }
                .store(in: &subscriptions)
        }
        // Away comes without an event: input just stops. A look now and then
        // sends what waits once the minutes have passed.
        timer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.sync() }
        }
        listen()
        sync()
    }

    /// Stops with the Sessions section; messages already sent stay as they are.
    func stop() {
        guard running else { return }
        running = false
        subscriptions.removeAll()
        timer?.invalidate()
        timer = nil
        poll?.cancel()
        poll = nil
        endsReady = false
    }

    private func settingsChanged() {
        let next = PhoneSettings.load()
        guard next != settings else { return }
        let wasActive = active
        settings = next
        if !active, wasActive { endsReady = false }
        listen()
        sync()
    }

    // MARK: Settings actions

    /// Checks a token BotFather gave and keeps it in the keychain.
    func useToken(_ raw: String) async {
        let token = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard TelegramAPI.validToken(token) else {
            problem = PhoneApprovalStrings.invalidToken
            return
        }
        checking = true
        defer { checking = false }
        let api = TelegramAPI(token: token)
        do {
            let me = try await api.call("getMe") as? [String: Any]
            guard let name = me?["username"] as? String, !name.isEmpty else { throw TelegramAPI.Failure.unauthorized }
            guard KeychainSecret.write(token, Self.tokenName, label: PhoneApprovalStrings.keychainLabel) else {
                problem = PhoneApprovalStrings.keychainFailed
                return
            }
            self.api?.close()
            self.api = api
            self.token = token
            bot = name
            UserDefaults.standard.set(name, forKey: DefaultsKey.notchSessionsPhoneBot)
            problem = nil
            startLinking()
        } catch {
            api.close()
            problem = (error as? TelegramAPI.Failure) == .network ? PhoneApprovalStrings.offline : PhoneApprovalStrings.invalidToken
        }
    }

    /// A new one-time code; the bot links the chat that sends it.
    func startLinking() {
        guard token != nil, link == nil else { return }
        linking = Linking(code: PhoneLinkCode.make(), expires: Date().addingTimeInterval(PhoneLinkCode.validFor))
        problem = nil
        listen()
    }

    func cancelLinking() {
        linking = nil
        listen()
    }

    var linkURL: String? {
        guard let bot, let linking else { return nil }
        return PhoneLinkCode.link(bot: bot, code: linking.code)
    }

    /// Unlinks the phone and forgets the bot: nothing more is sent or read.
    func unlink() {
        poll?.cancel()
        poll = nil
        outbox.removeAll()
        owed = [:]
        api?.close()
        api = nil
        token = nil
        KeychainSecret.delete(Self.tokenName)
        PhoneLink.clear()
        UserDefaults.standard.set(false, forKey: DefaultsKey.notchSessionsPhoneEnabled)
        link = nil
        bot = nil
        linking = nil
        problem = nil
        sent = [:]
        actions = [:]
        messageSessions = [:]
        messageOrder = []
        endsReady = false
    }

    func sendTest() {
        guard let link else { return }
        enqueue { [self] in
            _ = try? await call("sendMessage", message(chat: link.chat, text: PhoneMessages.test, buttons: []))
        }
    }

    // MARK: Reading updates

    private func listen() {
        if !listens {
            poll?.cancel()
            poll = nil
            return
        }
        guard poll == nil, let api else { return }
        pollGeneration += 1
        let generation = pollGeneration
        poll = Task { [weak self] in
            var backoff: TimeInterval = 2
            while !Task.isCancelled {
                guard let self, self.listens else { break }
                do {
                    let result = try await api.call("getUpdates", ["offset": self.offset, "timeout": 50,
                                                                   "allowed_updates": ["message", "callback_query"]], timeout: 65)
                    guard !Task.isCancelled else { break }
                    backoff = 2
                    if self.problem == PhoneApprovalStrings.offline || self.problem == PhoneApprovalStrings.conflict { self.problem = nil }
                    for update in result as? [[String: Any]] ?? [] {
                        if let id = Self.int(update["update_id"]) { self.offset = max(self.offset, id + 1) }
                        self.handle(update)
                    }
                } catch TelegramAPI.Failure.unauthorized {
                    self.problem = PhoneApprovalStrings.revoked
                    break
                } catch TelegramAPI.Failure.conflict {
                    self.problem = PhoneApprovalStrings.conflict
                    try? await Task.sleep(for: .seconds(30))
                } catch TelegramAPI.Failure.tooMany(let wait) {
                    try? await Task.sleep(for: .seconds(wait))
                } catch {
                    if (error as? TelegramAPI.Failure) == .network { self.problem = PhoneApprovalStrings.offline }
                    try? await Task.sleep(for: .seconds(backoff))
                    backoff = min(backoff * 2, 60)
                }
            }
            if let self, self.pollGeneration == generation { self.poll = nil }
        }
    }

    private func handle(_ update: [String: Any]) {
        if let message = update["message"] as? [String: Any] {
            handleMessage(message)
        } else if let query = update["callback_query"] as? [String: Any] {
            handleTap(query)
        }
    }

    private func handleMessage(_ message: [String: Any]) {
        guard let chat = message["chat"] as? [String: Any], chat["type"] as? String == "private",
              let chatID = Self.int(chat["id"]), let from = message["from"] as? [String: Any],
              let userID = Self.int(from["id"]), from["is_bot"] as? Bool != true else { return }
        let text = message["text"] as? String ?? ""
        if var linking, link == nil {
            guard let code = PhoneLinkCode.code(in: text) else { return }
            if Date() > linking.expires {
                self.linking = nil
                problem = PhoneApprovalStrings.codeExpired
                listen()
            } else if code == linking.code {
                linked(PhoneLink(chat: chatID, user: userID, name: Self.name(from)))
            } else {
                linking.tries += 1
                if linking.tries >= PhoneLinkCode.tries {
                    self.linking = nil
                    problem = PhoneApprovalStrings.tooManyTries
                    listen()
                } else {
                    self.linking = linking
                }
                say(PhoneMessages.wrongCode, to: chatID)
            }
            return
        }
        // Anyone but the linked person, or a group, is not answered at all.
        guard active, let link, chatID == link.chat, userID == link.user else { return }
        // Nothing typed while Openbar was not running reaches a session later.
        let date = Date(timeIntervalSince1970: (message["date"] as? Double) ?? 0)
        guard date >= startedAt.addingTimeInterval(-120) else { return }
        let replyTo = (message["reply_to_message"] as? [String: Any]).flatMap { Self.int($0["message_id"]) }
        typed(text, replyTo: replyTo)
    }

    private func linked(_ link: PhoneLink) {
        link.save()
        self.link = link
        linking = nil
        problem = nil
        UserDefaults.standard.set(true, forKey: DefaultsKey.notchSessionsPhoneEnabled)
        settings = PhoneSettings.load()
        endsReady = false
        say(PhoneMessages.linked(computer: Self.computerName), to: link.chat)
        listen()
        sync()
    }

    /// Text the linked person typed: their own answer to a question, or a
    /// reply into a session.
    private func typed(_ raw: String, replyTo: Int64?) {
        let text = String(raw.trimmingCharacters(in: .whitespacesAndNewlines).prefix(4000))
        guard !text.isEmpty, let link else { return }
        if text.hasPrefix("/") {
            if text.lowercased().hasPrefix("/start") { say(PhoneMessages.linked(computer: Self.computerName), to: link.chat) }
            return
        }
        let service = ClaudeSessionService.shared
        if let replyTo, let key = sent.first(where: { $0.value.message == replyTo })?.key, var entry = sent[key],
           !entry.settled, !entry.answering,
           let ask = service.sessions.first(where: { $0.id == entry.session })?.ask(entry.request), ask.asksQuestions {
            // The first question still open takes the typed answer.
            let open = ask.questions.indices.first { entry.picked[$0]?.isEmpty != false && entry.typed[$0] == nil } ?? 0
            entry.typed[open] = text
            sent[key] = entry
            if let answers = PhoneMessages.answers(ask, picked: entry.picked, typed: entry.typed) {
                answer(key, .answer(answers), outcome: .answered(Self.labels(answers, ask)))
            } else {
                say(PhoneApprovalStrings.pickTheRest, to: link.chat)
            }
            return
        }
        let target: String?
        if let replyTo {
            guard let session = messageSessions[replyTo] else { return say(PhoneMessages.earlierRun, to: link.chat) }
            target = PhoneReplyTarget.session(replyingTo: session, sessions: service.sessions)
        } else {
            target = PhoneReplyTarget.session(replyingTo: nil, sessions: service.sessions)
        }
        guard let target, let session = service.sessions.first(where: { $0.id == target }) else {
            let any = service.sessions.contains { NotchClaudeSessionsSupport.canReply($0) }
            return say(any ? PhoneMessages.whichSession : PhoneMessages.noSession, to: link.chat)
        }
        let name = service.displayName(session)
        service.send(text, to: session) { [weak self] failure in
            MainActor.assumeIsolated {
                self?.say(failure.map { PhoneMessages.replyFailed($0, name: name) } ?? PhoneMessages.sent(to: name), to: link.chat)
            }
        }
    }

    private func handleTap(_ query: [String: Any]) {
        guard let id = query["id"] as? String, let link,
              let from = query["from"] as? [String: Any], Self.int(from["id"]) == link.user,
              let message = query["message"] as? [String: Any],
              let chat = message["chat"] as? [String: Any], Self.int(chat["id"]) == link.chat else { return }
        guard let data = query["data"] as? String, let tap = PhoneCallback.parse(data) else { return reply(to: id) }
        guard tap.run == run else { return reply(to: id, PhoneMessages.earlierRun) }
        guard active, let found = actions[tap.number], var entry = sent[found.key], !entry.settled, !entry.answering,
              let session = ClaudeSessionService.shared.sessions.first(where: { $0.id == entry.session }),
              let ask = session.ask(entry.request) else { return reply(to: id, PhoneMessages.settled) }
        reply(to: id)
        let key = found.key
        switch found.action {
        case .allow:
            answer(key, .allow, outcome: PhoneMessages.isPlan(ask) ? .planApproved(nil) : .allowed)
        case .always:
            if session.codexServer {
                answer(key, .allow, autoAllowTurn: true, outcome: .always)
            } else {
                answer(key, .allowAlways, outcome: .always)
            }
        case .deny:
            answer(key, .deny, outcome: .denied)
        case .keepPlanning:
            answer(key, .deny, outcome: .keptPlanning)
        case .skip:
            answer(key, .deny, outcome: .skipped)
        case .approvePlan(let mode):
            answer(key, mode.map { .approvePlan($0) } ?? .allow, outcome: .planApproved(mode.map(PhoneMessages.planLabel)))
        case .pick(let q, let o):
            guard ask.questions.indices.contains(q) else { return }
            if PhoneMessages.answersOnTap(ask) {
                guard let answers = PhoneMessages.answers(ask, picked: [q: [o]]) else { return }
                answer(key, .answer(answers), outcome: .answered(Self.labels(answers, ask)))
                return
            }
            var chosen = entry.picked[q] ?? []
            if ask.questions[q].multiSelect {
                if chosen.contains(o) { chosen.remove(o) } else { chosen.insert(o) }
            } else {
                chosen = [o]
                entry.typed[q] = nil
            }
            entry.picked[q] = chosen
            sent[key] = entry
            redraw(key, ask: ask, session: session)
        case .send:
            guard let answers = PhoneMessages.answers(ask, picked: entry.picked, typed: entry.typed) else { return }
            answer(key, .answer(answers), outcome: .answered(Self.labels(answers, ask)))
        }
    }

    /// Answers the request as the island would, and tells on the phone how it went.
    private func answer(_ key: String, _ decision: ClaudeHookMessage.Decision, autoAllowTurn: Bool = false, outcome: PhoneOutcome) {
        guard var entry = sent[key] else { return }
        entry.answering = true
        sent[key] = entry
        ClaudeSessionService.shared.decide(entry.session, request: entry.request, decision, revision: entry.revision,
                                           autoAllowTurn: autoAllowTurn, origin: .phone) { [weak self] answered in
            MainActor.assumeIsolated {
                guard let self, var entry = self.sent[key] else { return }
                entry.answering = false
                self.sent[key] = entry
                if answered {
                    Self.log.info("answered a request from the phone")
                    self.settle(key, outcome)
                } else {
                    let changed = ClaudeSessionService.shared.sessions.first { $0.id == entry.session }?.ask(entry.request)
                        .map { Self.revision($0) != entry.revision } ?? false
                    self.settle(key, changed ? .changed : .failed)
                }
            }
        }
    }

    // MARK: Sending

    /// Looks at what waits and sends, updates or settles messages.
    func sync() {
        guard active else { return }
        let service = ClaudeSessionService.shared
        let sessions = service.sessions.filter { $0.id != ClaudeSessionSample.id }
        var live: [String: (session: ClaudeSession, ask: ClaudePermission)] = [:]
        for session in sessions where !service.autoAllowed.contains(session.id) {
            for ask in session.asks { live[Self.key(session.id, ask.requestID)] = (session, ask) }
        }
        for (key, entry) in sent where !entry.answering {
            if let now = live[key] {
                if !entry.settled, Self.revision(now.ask) != entry.revision {
                    // The same request now says something else: the old
                    // message stops, and a new one goes out below.
                    settle(key, .changed)
                    sent[key] = nil
                }
            } else {
                if !entry.settled { settle(key, sessions.contains { $0.id == entry.session } ? .answeredElsewhere : .gone) }
                sent[key] = nil
            }
        }
        let away = PhoneAway.sends(settings, locked: Self.locked(), idle: Self.idle())
        var fresh = false
        if away {
            for (key, item) in live where sent[key] == nil {
                // A moment first, for what the island answers by itself.
                if Date().timeIntervalSince(item.ask.since) < 1.5 { fresh = true; continue }
                send(item.ask, of: item.session, key: key)
            }
        }
        if fresh { scheduleResync() }
        // Ends seen when the bridge starts, or while the person was here,
        // are never sent later.
        guard settings.tellEnds else { endsReady = false; return }
        var current: Set<String> = []
        for session in sessions {
            guard let event = NotchSessionWaitingSound.terminal(session) else { continue }
            let id = "\(session.id)\u{1}\(event.kind.rawValue)\u{1}\(event.id)"
            current.insert(id)
            guard endsReady, away, !ends.contains(id) else { continue }
            let message = PhoneMessages.ended(event.kind, of: session, name: service.displayName(session), hideCode: settings.hideCode)
            post(message, about: session.id)
        }
        ends.formUnion(current)
        if ends.count > 2048 { ends = current }
        endsReady = true
    }

    private func scheduleResync() {
        resync?.cancel()
        let work = DispatchWorkItem { [weak self] in MainActor.assumeIsolated { self?.sync() } }
        resync = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.6, execute: work)
    }

    private func send(_ ask: ClaudePermission, of session: ClaudeSession, key: String) {
        guard let link else { return }
        let message = PhoneMessages.ask(ask, of: session, name: ClaudeSessionService.shared.displayName(session),
                                        hideCode: settings.hideCode)
        serial += 1
        let serial = serial
        var entry = Sent(serial: serial, session: session.id, request: ask.requestID, revision: Self.revision(ask), text: message.text)
        let keyboard = issue(message.buttons, key: key, entry: &entry)
        sent[key] = entry
        let quiet = PhoneAway.quiet(settings, minute: PhoneAway.minute(of: Date()))
        enqueue { [self] in
            var body = self.message(chat: link.chat, text: message.text, buttons: keyboard)
            body["disable_notification"] = quiet
            guard let result = try? await call("sendMessage", body) as? [String: Any], let id = Self.int(result["message_id"]) else {
                // Not sent: the next look tries again.
                owed[serial] = nil
                if sent[key]?.serial == serial, sent[key]?.settled == false { sent[key] = nil }
                return
            }
            remember(id, session: session.id)
            if var entry = sent[key], entry.serial == serial {
                entry.message = id
                sent[key] = entry
            }
            if let text = owed.removeValue(forKey: serial) { await edit(id, text: text) }
        }
    }

    /// A message that isn't a request, as a session's end.
    private func post(_ message: PhoneMessage, about session: String) {
        guard let link else { return }
        let quiet = PhoneAway.quiet(settings, minute: PhoneAway.minute(of: Date()))
        enqueue { [self] in
            var body = self.message(chat: link.chat, text: message.text, buttons: [])
            body["disable_notification"] = quiet
            if let result = try? await call("sendMessage", body) as? [String: Any], let id = Self.int(result["message_id"]) {
                remember(id, session: session)
            }
        }
    }

    /// Ends a request's message with how it went, and takes its buttons away.
    private func settle(_ key: String, _ outcome: PhoneOutcome) {
        guard var entry = sent[key], !entry.settled else { return }
        entry.settled = true
        for number in entry.numbers { actions[number] = nil }
        entry.numbers = []
        let text = entry.text + "\n\n" + PhoneMessages.outcome(outcome, at: Date())
        sent[key] = entry
        if let id = entry.message {
            enqueue { [self] in await edit(id, text: text) }
        } else {
            owed[entry.serial] = text
        }
    }

    /// New buttons for a question whose picks changed.
    private func redraw(_ key: String, ask: ClaudePermission, session: ClaudeSession) {
        guard var entry = sent[key], let id = entry.message, let link else { return }
        for number in entry.numbers { actions[number] = nil }
        entry.numbers = []
        let buttons = PhoneMessages.buttons(ask, of: session, hideCode: settings.hideCode, picked: entry.picked)
        let keyboard = issue(buttons, key: key, entry: &entry)
        sent[key] = entry
        enqueue { [self] in
            _ = try? await call("editMessageReplyMarkup", ["chat_id": link.chat, "message_id": id,
                                                          "reply_markup": ["inline_keyboard": keyboard]])
        }
    }

    private func edit(_ id: Int64, text: String) async {
        guard let link else { return }
        var body = message(chat: link.chat, text: text, buttons: [])
        body["message_id"] = id
        _ = try? await call("editMessageText", body)
    }

    private func say(_ text: String, to chat: Int64) {
        enqueue { [self] in _ = try? await call("sendMessage", message(chat: chat, text: text, buttons: [])) }
    }

    private func reply(to query: String, _ text: String? = nil) {
        guard let api else { return }
        var body: [String: Any] = ["callback_query_id": query]
        if let text { body["text"] = text }
        Task { _ = try? await api.call("answerCallbackQuery", body) }
    }

    private func message(chat: Int64, text: String, buttons: [[[String: Any]]]) -> [String: Any] {
        ["chat_id": chat, "text": text, "parse_mode": "HTML",
         "link_preview_options": ["is_disabled": true],
         "reply_markup": ["inline_keyboard": buttons]]
    }

    /// Numbers the buttons of a message for this run.
    private func issue(_ rows: [[PhoneButton]], key: String, entry: inout Sent) -> [[[String: Any]]] {
        rows.map { row in
            row.map { button in
                nextNumber += 1
                actions[nextNumber] = (key, button.action)
                entry.numbers.append(nextNumber)
                return ["text": button.title, "callback_data": PhoneCallback.data(run: run, number: nextNumber)]
            }
        }
    }

    private func remember(_ message: Int64, session: String) {
        messageSessions[message] = session
        messageOrder.append(message)
        if messageOrder.count > 500 { messageSessions[messageOrder.removeFirst()] = nil }
    }

    /// One call at a time, a moment apart, as Telegram asks of bots.
    private func enqueue(_ job: @escaping () async -> Void) {
        outbox.append(job)
        guard !pumping else { return }
        pumping = true
        Task { [self] in
            while !outbox.isEmpty {
                let next = outbox.removeFirst()
                await next()
                try? await Task.sleep(for: .milliseconds(350))
            }
            pumping = false
        }
    }

    /// A call that waits out Telegram's "too many requests" once.
    private func call(_ method: String, _ body: [String: Any]) async throws -> Any {
        guard let api else { throw TelegramAPI.Failure.unauthorized }
        do {
            return try await api.call(method, body)
        } catch TelegramAPI.Failure.tooMany(let wait) {
            try? await Task.sleep(for: .seconds(wait))
            return try await api.call(method, body)
        } catch TelegramAPI.Failure.unauthorized {
            problem = PhoneApprovalStrings.revoked
            throw TelegramAPI.Failure.unauthorized
        }
    }

    // MARK: Helpers

    private static func key(_ session: String, _ request: String) -> String { session + "\u{1}" + request }

    private static func revision(_ ask: ClaudePermission) -> String? { ask.content.map { $0.revision(ask) } }

    private static func int(_ value: Any?) -> Int64? {
        if let number = value as? NSNumber { return number.int64Value }
        return nil
    }

    private static func name(_ user: [String: Any]) -> String {
        if let username = user["username"] as? String, !username.isEmpty { return "@" + username }
        let name = [user["first_name"] as? String, user["last_name"] as? String].compactMap { $0 }.joined(separator: " ")
        return name.isEmpty ? "Telegram" : name
    }

    private static func labels(_ answers: [String: [String]], _ ask: ClaudePermission) -> [String] {
        ask.questions.flatMap { answers[$0.question] ?? [] }
    }

    private static var computerName: String {
        (SCDynamicStoreCopyComputerName(nil, nil) as String?) ?? "this Mac"
    }

    private static func locked() -> Bool {
        KeepAwakeAutomationSupport.isScreenLocked(sessionDictionary: CGSessionCopyCurrentDictionary() as? [String: Any])
    }

    private static func idle() -> TimeInterval {
        CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: CGEventType(rawValue: ~0)!)
    }
}
