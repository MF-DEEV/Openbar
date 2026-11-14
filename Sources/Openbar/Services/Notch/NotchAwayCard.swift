// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import Combine
import CoreGraphics

/// The card on screen, and its clock: it closes itself unless the pointer
/// rests on it.
@MainActor
final class NotchAwayCard: ObservableObject {
    static let shared = NotchAwayCard()
    static let closeAfter: TimeInterval = 15

    @Published private(set) var lines: [NotchAway.Line] = []
    @Published private(set) var since: Date?
    @Published private(set) var left: TimeInterval = closeAfter
    var held = false
    private var timer: Timer?

    var isShown: Bool { since != nil }

    func present(_ lines: [NotchAway.Line], since: Date) {
        self.lines = lines
        self.since = since
        left = Self.closeAfter
        held = false
        ClaudeSessionService.shared.focus(nil)
        ClaudeSessionService.shared.composingSession = false
        NotchService.shared.open(.sessions, takeFocus: false)
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in
            MainActor.assumeIsolated {
                let card = NotchAwayCard.shared
                guard !card.held else { return }
                card.left = max(0, card.left - 0.1)
                if card.left <= 0 { card.close() }
            }
        }
    }

    /// Closes the card, and the island with it unless a tile opened a session.
    func close(collapse: Bool = true) {
        guard isShown else { return }
        timer?.invalidate()
        timer = nil
        since = nil
        lines = []
        if collapse { NotchService.shared.collapse() }
    }

    func open(_ id: String) {
        close(collapse: false)
        ClaudeSessionService.shared.focus(id)
    }
}

/// When the person steps away and comes back. Away begins with the lock
/// screen or after the set minutes without input; it ends with the unlock or
/// the first input after. The sessions as they stood when the person was
/// last at the Mac are kept, so the card tells what changed since, the
/// minutes it took to count as away included.
@MainActor
final class NotchAwayWatcher {
    static let shared = NotchAwayWatcher()

    /// How often input idleness is read; more often while away, to catch the return.
    private static let poll: TimeInterval = 20
    private static let pollAway: TimeInterval = 3
    /// Input this recent counts as the person being here.
    private static let here: TimeInterval = 30

    private(set) var seen: NotchAway.Mark?
    private(set) var away: NotchAway.Mark?
    private var timer: Timer?
    private var observers: [NSObjectProtocol] = []

    func start() {
        guard observers.isEmpty else { return }
        let center = DistributedNotificationCenter.default()
        observers.append(center.addObserver(forName: .init("com.apple.screenIsLocked"), object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { NotchAwayWatcher.shared.wentAway() }
        })
        observers.append(center.addObserver(forName: .init("com.apple.screenIsUnlocked"), object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { NotchAwayWatcher.shared.cameBack() }
        })
        seen = mark()
        schedule(Self.poll)
    }

    private func mark() -> NotchAway.Mark {
        NotchAway.mark(ClaudeSessionService.shared.sessions, at: Date())
    }

    /// The person stepped away; the last mark from while they were here stands.
    func wentAway() {
        guard away == nil else { return }
        away = seen ?? mark()
        schedule(Self.pollAway)
    }

    /// The person is back: the card, when anything changed and it is wanted.
    func cameBack() {
        guard let since = away else { return }
        away = nil
        seen = mark()
        schedule(Self.poll)
        guard UserDefaults.standard.object(forKey: DefaultsKey.notchSessionsAwayCard) as? Bool ?? true else { return }
        // A moment after, so the island has heard of the unlock and may open.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
            let lines = NotchAway.lines(since: since, sessions: ClaudeSessionService.shared.sessions)
            if !lines.isEmpty { NotchAwayCard.shared.present(lines, since: since.at) }
        }
    }

    private func schedule(_ interval: TimeInterval) {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: false) { _ in
            MainActor.assumeIsolated { NotchAwayWatcher.shared.tick() }
        }
    }

    private func tick() {
        let anyInput = CGEventType(rawValue: ~0)!
        let idle = CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: anyInput)
        let minutes = UserDefaults.standard.object(forKey: DefaultsKey.notchSessionsAwayMinutes) as? Int ?? 10
        if away != nil, idle < Self.here {
            cameBack()
            return
        } else if away == nil, idle >= Double(max(1, minutes)) * 60 {
            wentAway()
            return
        } else if away == nil, idle < Self.here {
            seen = mark()
        }
        schedule(away == nil ? Self.poll : Self.pollAway)
    }
}
