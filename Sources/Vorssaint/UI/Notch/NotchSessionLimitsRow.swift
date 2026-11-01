// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// Claude's and Codex's plan limits above the Sessions list: each agent's
/// session and weekly windows as a thin meter, a share and the time until it
/// renews. Quiet while there is room, colored as a window nears its end. The
/// reading is the AI section's; nothing shows until it has one.
struct NotchSessionLimitsRow: View {
    let text: NotchClaudeSessionStrings
    @ObservedObject private var usage = AgentUsageService.shared
    @ObservedObject private var l10n = L10n.shared
    @AppStorage(DefaultsKey.notchSessionsLimits) private var shows = true
    @AppStorage(DefaultsKey.notchAgentsLimitDisplay) private var display = NotchAgentLimitDisplay.remaining.rawValue
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    static let height: CGFloat = 24

    var body: some View {
        // A minute is the finest a countdown here shows.
        TimelineView(.periodic(from: .now, by: 30)) { context in
            let groups = shows ? NotchSessionLimitsSupport.groups(usage.snapshot, now: context.date) : []
            if !groups.isEmpty {
                ViewThatFits(in: .horizontal) {
                    row(groups, now: context.date, resets: true)
                    row(groups, now: context.date, resets: false)
                    row(groups.map { NotchSessionLimitsSupport.Group(provider: $0.provider, windows: Array($0.windows.prefix(1))) },
                        now: context.date, resets: false)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .transition(reduceMotion ? .opacity : .move(edge: .top).combined(with: .opacity))
                .accessibilityElement(children: .combine)
                .accessibilityLabel(text.limitsTitle)
            }
        }
        .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.86),
                   value: shows && !usage.snapshot.limits.isEmpty)
    }

    private var locale: Locale { l10n.language.formattingLocale() }
    private var agentText: NotchAgentStrings { FeatureStrings.notchAgents(l10n.language) }

    private func row(_ groups: [NotchSessionLimitsSupport.Group], now: Date, resets: Bool) -> some View {
        HStack(spacing: 6) {
            ForEach(groups) { group in
                chip(group, now: now, resets: resets)
            }
        }
        .fixedSize()
    }

    private func chip(_ group: NotchSessionLimitsSupport.Group, now: Date, resets: Bool) -> some View {
        let stale = NotchAgentSupport.limitsAreStale(usage.snapshot.limits[group.provider], provider: group.provider,
                                                     account: false, now: now)
        return HStack(spacing: 8) {
            NotchSessionAgentGlyph(provider: group.provider == .codex ? .codex : .claude, size: 9, working: false)
            ForEach(group.windows) { window in
                meter(window, provider: group.provider, now: now, resets: resets)
            }
        }
        .padding(.leading, 5)
        .padding(.trailing, 9)
        .frame(height: Self.height)
        .background(.white.opacity(0.06), in: Capsule(style: .continuous))
        .opacity(stale ? 0.6 : 1)
    }

    private func meter(_ window: AgentLimitWindow, provider: AgentProvider, now: Date, resets: Bool) -> some View {
        let used = display == NotchAgentLimitDisplay.used.rawValue
        let fraction = used ? window.usedFraction : window.remainingFraction
        let tint = agentLimitTint(provider, usedFraction: window.usedFraction)
        // Under the warning colors it stays quiet; past them the share takes the color.
        let urgent = tint != provider.tint
        let label = NotchSessionLimitsSupport.label(window, session: agentText.session, weekly: agentText.weekly, locale: locale)
        let reset = AgentLimitSupport.displayedReset(window, now: now)
        return HStack(spacing: 4) {
            Text(label)
                .font(.system(size: 9.5, weight: .medium))
                .foregroundStyle(.white.opacity(0.55))
            NotchAgentMeter(value: fraction, tint: tint, height: 3, dimmed: !urgent)
                .frame(width: 24)
            Text(AgentFormat.percent(fraction, locale: locale))
                .font(.system(size: 10.5, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .foregroundStyle(urgent ? tint : .white.opacity(0.85))
                .contentTransition(.numericText())
            if resets, let reset {
                Label(countdown(to: reset, now: now), systemImage: "arrow.clockwise")
                    .labelStyle(NotchAgentInlineLabel())
                    .font(.system(size: 9.5))
                    .foregroundStyle(.white.opacity(0.45))
            }
        }
        .lineLimit(1)
        .help(help(window, provider: provider, label: label, reset: reset))
    }

    /// Days and hours, or hours and minutes, as the AI page counts them down.
    private func countdown(to date: Date, now: Date) -> String {
        let seconds = date.timeIntervalSince(now)
        return AgentFormat.duration(max(60, seconds), locale: locale, units: 1)
    }

    private func help(_ window: AgentLimitWindow, provider: AgentProvider, label: String, reset: Date?) -> String {
        var parts = ["\(provider.displayName) · \(label)", agentText.usedShare(AgentFormat.percent(window.usedFraction, locale: locale)),
                     agentText.left(AgentFormat.percent(window.remainingFraction, locale: locale))]
        if let reset {
            parts.append(text.resets(reset.formatted(Date.FormatStyle(date: .abbreviated, time: .shortened).locale(locale))))
        }
        return parts.joined(separator: " · ")
    }
}
