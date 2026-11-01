// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The plan limits the Sessions section shows on the island. They come
/// from the reading the AI section keeps (`AgentUsageService`); nothing here
/// reads a file or asks an agent.
enum NotchSessionLimitsSupport {
    /// The agents Sessions follows that report plan limits.
    static let providers: [AgentProvider] = [.claude, .codex]

    /// Whether Sessions shows the limits: on unless turned off, with the AI
    /// feature installed, since its reader keeps them.
    static func isEnabled(in defaults: UserDefaults = .standard) -> Bool {
        NotchClaudeSessionsSupport.isEnabled(in: defaults) && AppFeature.notchAgents.isAvailable(in: defaults)
            && (defaults.object(forKey: DefaultsKey.notchSessionsLimits) as? Bool ?? true)
    }

    /// Whether the reading runs for Sessions alone: the AI section is off,
    /// so only Claude's and Codex's limits are read, and only the limit
    /// warnings among its notices are told.
    static func readsAlone(in defaults: UserDefaults = .standard) -> Bool {
        isEnabled(in: defaults) && !NotchAgentSupport.isEnabled(in: defaults)
    }

    /// Whether anything should read agent usage: the AI section, or Sessions
    /// for its limits.
    static func usageWanted(in defaults: UserDefaults = .standard) -> Bool {
        NotchAgentSupport.isEnabled(in: defaults) || isEnabled(in: defaults)
    }

    // MARK: Strip

    /// The limit the closed strip shows beside the counts: the window the
    /// person chose for the AI strip, or the most used one, among the agents
    /// with sessions listed, and only once it passes the warning threshold.
    /// A chosen window still under it gives way to one that is past it, as
    /// a weekly cap reached with room left in the session window.
    static func stripLimit(_ snapshot: AgentUsageSnapshot, agents: Set<AgentProvider>, focus: NotchAgentLimitFocus,
                           threshold: Double, now: Date) -> (provider: AgentProvider, window: AgentLimitWindow)? {
        var scoped = snapshot
        scoped.limits = snapshot.limits.filter { agents.contains($0.key) && providers.contains($0.key) }
        let chosen = NotchAgentSupport.restingLimit(scoped, focus: focus, now: now)
        let binding = NotchAgentSupport.restingLimit(scoped, focus: .mostUsed, now: now)
        return [chosen, binding].compactMap { $0 }.first { $0.window.usedPercent >= threshold }
    }

    /// The share the strip writes for it, as the person reads limits.
    static func stripShare(_ window: AgentLimitWindow, display: NotchAgentLimitDisplay,
                           locale: Locale = .autoupdatingCurrent) -> String {
        AgentFormat.percent(display == .used ? window.usedFraction : window.remainingFraction, locale: locale)
    }

    // MARK: Stopped sessions

    /// A window this full has run out, allowing for rounding in the reading.
    static let spentPercent = 99.5

    /// When the limit that stopped a session lifts: as the agent said, or
    /// else the latest reset among its agent's spent windows of that kind,
    /// since the work waits for all of them. Nil once that time has passed
    /// or while nothing tells it.
    static func reset(of session: ClaudeSession, snapshot: AgentUsageSnapshot, now: Date) -> Date? {
        guard let stop = session.limitStop else { return nil }
        if let resets = stop.resetsAt { return resets > now ? resets : nil }
        let windows = snapshot.limits[session.provider.agent]?.windows ?? []
        return windows.filter { $0.usedPercent >= spentPercent && (stop.kind == nil || $0.kind == stop.kind) }
            .compactMap { AgentLimitSupport.displayedReset($0, now: now) }.max()
    }

    /// A reset time as short as it can be told: the time today, the weekday
    /// and time within the week, the date and time after that.
    static func resetText(_ date: Date, now: Date, locale: Locale, calendar: Calendar = .autoupdatingCurrent) -> String {
        var calendar = calendar
        calendar.locale = locale
        var time = Date.FormatStyle(date: .omitted, time: .shortened).locale(locale)
        time.calendar = calendar
        time.timeZone = calendar.timeZone
        if calendar.isDate(date, inSameDayAs: now) { return date.formatted(time) }
        let days = calendar.dateComponents([.day], from: calendar.startOfDay(for: now), to: calendar.startOfDay(for: date)).day ?? 0
        var style = days < 7
            ? Date.FormatStyle(date: .omitted, time: .shortened).weekday(.abbreviated).locale(locale)
            : Date.FormatStyle(date: .abbreviated, time: .shortened).locale(locale)
        style.calendar = calendar
        style.timeZone = calendar.timeZone
        return date.formatted(style)
    }

    // MARK: Warnings

    /// The sessions of an agent still under way, at work or waiting on the
    /// person, which a warning about that agent's limit counts.
    static func running(_ sessions: [ClaudeSession], provider: AgentProvider) -> Int {
        sessions.filter { $0.provider.agent == provider && ($0.phase.isActive || $0.phase.isWaiting) }.count
    }
}
