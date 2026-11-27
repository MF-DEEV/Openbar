// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import Combine
import os.log

/// The sound once for a new request (#167, #210, #262) and the reminders of
/// a wait nobody reacted to (#231), for every place the sessions show: the
/// island, their menu bar item, or both (#331). One runner, so a request
/// sounds once whichever of them shows it, and both keep working with the
/// island turned off.
final class SessionAlerts {
    static let shared = SessionAlerts()
    static let log = Logger(subsystem: Bundle.main.bundleIdentifier ?? "openbar", category: "waiting-sound")

    private var running = false
    private var subscriptions = Set<AnyCancellable>()
    private var unlockObserver: NSObjectProtocol?
    private var waitingSound = NotchSessionWaitingSound()
    private var soundPending: [NotchSessionWaitingSound.Event] = []
    private var soundWork: DispatchWorkItem?
    private var soundChecking = false
    private var reminders = NotchSessionReminders()
    private var reminderChecking = false
    private var reminderWork: DispatchWorkItem?
    /// The terminals and Control Center are asked off the main thread.
    private let checks = DispatchQueue(label: "com.openbar.waiting-sound", qos: .userInitiated)

    private init() {}

    /// Runs while the sessions are followed anywhere.
    func sync() {
        guard NotchClaudeSessionsSupport.isEnabled() else { stop(); return }
        guard !running else { return }
        running = true
        // Requests already waiting when the sessions come up stay quiet.
        waitingSound = NotchSessionWaitingSound()
        ClaudeSessionService.shared.$sessions
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in self?.sessionsChanged() }
            .store(in: &subscriptions)
        Timer.publish(every: NotchSessionReminders.checkEvery, on: .main, in: .common).autoconnect()
            .sink { [weak self] _ in self?.checkReminders() }
            .store(in: &subscriptions)
        // Without the island nothing else hears the unlock; with it, the
        // island says so itself once it can show again.
        unlockObserver = DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name("com.apple.screenIsUnlocked"), object: nil, queue: .main
        ) { [weak self] _ in
            guard !NotchService.shared.running else { return }
            self?.resume()
        }
    }

    func stop() {
        guard running else { return }
        running = false
        subscriptions.removeAll()
        if let unlockObserver { DistributedNotificationCenter.default().removeObserver(unlockObserver) }
        unlockObserver = nil
        soundWork?.cancel(); soundWork = nil
        soundPending.removeAll()
        reminderWork?.cancel(); reminderWork = nil
    }

    /// Back from the lock screen or sleep: what came meanwhile sounds now,
    /// and a reminder held through it comes a moment later.
    func resume() {
        guard running else { return }
        sessionsChanged()
        scheduleReminderCheck(after: NotchSessionReminders.afterUnlock)
    }

    /// Held while the Mac is locked or asleep. The island keeps that state
    /// while it runs; without it the session itself is asked.
    private var suspended: Bool {
        if NotchService.shared.running { return NotchService.shared.presentationSuspended }
        let locked = (CGSessionCopyCurrentDictionary() as? [String: Any])?["CGSSessionScreenIsLocked"] as? Bool ?? false
        return locked || !SessionActivity.shared.isActive
    }

    /// Whether the person already looks at the session: its page, or the
    /// list holding it, in the island or in the menu bar panel.
    private func inView(_ session: String) -> Bool {
        NotchService.shared.showsSessionInView(session) || SessionsMenuBar.shared.showsSessionInView(session)
    }

    // MARK: Sound

    /// A single notification for each one-second batch. Snapshot rows are
    /// already the parent's rows (#210), and all terminal facts come from the
    /// agent rather than tool result text. Settings and mute are read again
    /// after the terminal check so a late answer cannot play a stale sound.
    /// While the Mac is locked the readings wait, so what came meanwhile is
    /// new at the unlock.
    private func sessionsChanged() {
        guard running, !suspended else { return }
        soundPending.append(contentsOf: waitingSound.events(ClaudeSessionService.shared.sessions))
        if soundPending.count > 256 { soundPending.removeFirst(soundPending.count - 256) }
        scheduleSound()
    }

    private func scheduleSound() {
        guard !soundPending.isEmpty, soundWork == nil, !soundChecking else { return }
        let work = DispatchWorkItem { [weak self] in self?.flushSounds() }
        soundWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0, execute: work)
    }

    private func audible(_ event: NotchSessionWaitingSound.Event) -> Bool {
        guard running, NotchClaudeSessionsSupport.isEnabled(), !SessionSoundPreferences.muted(),
              SessionSoundPreferences.volume() > 0, SessionSoundPreferences.choice(event.kind) != "none",
              let session = ClaudeSessionService.shared.sessions.first(where: { $0.id == event.session }) else { return false }
        if inView(event.session) { return false }
        return NotchSessionWaitingSound.current(event, session)
    }

    private func flushSounds() {
        soundWork = nil
        soundChecking = true
        let batch = soundPending.filter(audible)
        soundPending.removeAll()
        let ids = Set(batch.map(\.session))
        guard !ids.isEmpty else { soundChecking = false; scheduleSound(); return }
        let sessions = ClaudeSessionService.shared.sessions.filter { ids.contains($0.id) }
        var decided = false
        let decide = { [weak self] (visible: Set<String>) in
            guard let self, !decided else { return }
            decided = true
            let choice = batch.filter { visible.contains($0.session) && self.audible($0) }
                .sorted { lhs, rhs in lhs.kind.priority == rhs.kind.priority ? lhs.at > rhs.at : lhs.kind.priority > rhs.kind.priority }
                .first
            if let choice {
                let name = SessionSoundPreferences.choice(choice.kind)
                let played = SessionSoundPlayer.play(name, volume: SessionSoundPreferences.volume())
                Self.log.info("session sound \(played ? "played" : "unavailable", privacy: .public): \(choice.kind.rawValue, privacy: .public)")
            }
            self.soundChecking = false
            self.scheduleSound()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + NotchSessionWaitingSound.terminalCheckLimit) {
            decide(ids)
        }
        checks.async {
            let visible = Set(sessions.filter { ClaudeTerminalFocus.state(of: $0) != .shown }.map(\.id))
            DispatchQueue.main.async { decide(visible) }
        }
    }

    // MARK: Reminders

    private func scheduleReminderCheck(after delay: TimeInterval) {
        reminderWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            self?.reminderWork = nil
            self?.checkReminders()
        }
        reminderWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    /// Reminds of the sessions that have waited long enough with nobody
    /// reacting (#231). A locked or asleep Mac waits, and so does Focus
    /// unless it was allowed; a session whose own tab is in front counts as
    /// seen. The terminals and Control Center are asked off the main thread.
    private func checkReminders() {
        guard running, !reminderChecking, !suspended else { return }
        let settings = NotchSessionReminders.Settings.load()
        let sessions = ClaudeSessionService.shared.sessions
        let due = reminders.due(sessions, settings: settings, now: Date())
        guard !due.isEmpty else { return }
        reminderChecking = true
        let ids = Set(due.map(\.session))
        let asked = sessions.filter { ids.contains($0.id) }
        var decided = false
        let decide = { [weak self] (shown: Set<String>, focus: Bool?) in
            guard let self, !decided else { return }
            decided = true
            self.reminderChecking = false
            self.finishReminder(due, settings: settings, shown: shown, focus: focus)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + NotchSessionReminders.checkLimit) {
            decide([], nil)
        }
        checks.async {
            let focus = settings.duringFocus ? nil : NotchFocusStatus.isOn()
            let shown = Set(asked.filter { ClaudeTerminalFocus.state(of: $0) == .shown }.map(\.id))
            DispatchQueue.main.async { decide(shown, focus) }
        }
    }

    private func finishReminder(_ due: [NotchSessionReminders.Wait], settings: NotchSessionReminders.Settings,
                                shown: Set<String>, focus: Bool?) {
        guard running, !suspended else { return }
        if NotchSessionReminders.heldByFocus(settings, focus: focus) {
            Self.log.info("reminder held: focus is on")
            return
        }
        // Answered while the terminals were asked: no reminder of it.
        let now = Date()
        let sessions = ClaudeSessionService.shared.sessions
        let live = due.filter { wait in
            sessions.first { $0.id == wait.session }
                .flatMap { NotchSessionReminders.wait(of: $0, now: now) }?.isSame(as: wait) == true
        }
        reminders.mark(live.filter { shown.contains($0.session) }, at: now)
        let remind = live.filter { !shown.contains($0.session) }
        if remind.count < live.count {
            Self.log.info("reminder skipped for \(live.count - remind.count, privacy: .public) session(s): own terminal in front")
        }
        guard !remind.isEmpty else { return }
        reminders.mark(remind, at: now)
        presentReminder(remind, sessions: sessions, now: now)
    }

    /// One reminder for every session in it: its sound, then whatever shows
    /// the sessions brings them up — the island, the menu bar, or both.
    private func presentReminder(_ waits: [NotchSessionReminders.Wait], sessions: [ClaudeSession], now: Date) {
        var sound = "none"
        if let kind = NotchSessionReminders.sound(for: waits), !SessionSoundPreferences.muted(),
           SessionSoundPreferences.volume() > 0 {
            let name = SessionSoundPreferences.choice(kind)
            if name != "none" {
                sound = SessionSoundPlayer.play(name, volume: SessionSoundPreferences.volume()) ? kind.rawValue : "unavailable"
            }
        }
        Self.log.info("reminder fired: \(waits.count, privacy: .public) session(s) [\(waits.map(\.kind.rawValue).joined(separator: ","), privacy: .public)], sound \(sound, privacy: .public)")
        if NotchService.shared.presentsSessions {
            NotchService.shared.presentSessionReminder(waits, sessions: sessions, now: now)
        }
        if NotchClaudeSessionsSupport.inMenuBar() {
            SessionsMenuBar.shared.remind(waits, sessions: sessions)
        }
    }
}
