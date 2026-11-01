// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The closed island while Claude Code and Codex sessions run: the most
/// pressing phase on one side of the camera, how many sessions are in each
/// phase on the other, the ones at work counted by agent. A session waiting
/// on the person shows first.
struct NotchClaudeSessionStrip: View {
    @ObservedObject var service: NotchService
    var displayGeometry: NotchGeometry? = nil
    @ObservedObject private var sessions = ClaudeSessionService.shared
    @ObservedObject private var usage = AgentUsageService.shared
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        NotchStripHold(sessions.counts, shows: !sessions.counts.isEmpty) { strip(counts: $0) }
    }

    private var limitShare: String? { NotchSessionLimitsSupport.currentStripLimit()?.share }

    @ViewBuilder private func strip(counts: ClaudeSessionCounts) -> some View {
        let geometry = displayGeometry ?? service.compactActivityGeometry
        let textSize = NotchAgentSupport.stripTextSize(height: geometry.compactActivityContentHeight)
        let iconInset = !geometry.compactActivityUsesFooter
            ? geometry.compactActivityEdgeInset(boxHeight: 18, radius: 9) : 0
        let textInset = !geometry.compactActivityUsesFooter
            ? geometry.compactActivityEdgeInset(boxHeight: textSize * 0.72, radius: 0) : 0
        HStack(spacing: 0) {
            Button(action: open) {
                NotchClaudeSessionStripMark(counts: counts, size: 14)
                    .padding(.leading, iconInset)
                    .frame(width: geometry.compactActivityWingWidth, height: geometry.compactActivityContentHeight,
                           alignment: .leading)
                    .contentShape(Rectangle())
            }
            Color.clear.frame(width: geometry.compactActivityCameraGap)
            Button(action: open) {
                NotchClaudeSessionStripCounts(counts: counts, size: textSize)
                    .padding(.trailing, textInset)
                    .frame(width: geometry.compactActivityWingWidth, height: geometry.compactActivityContentHeight,
                           alignment: .trailing)
                    .contentShape(Rectangle())
            }
        }
        .frame(height: geometry.compactActivityContentHeight)
        .padding(.horizontal, geometry.compactActivityHorizontalPadding)
        .padding(.top, geometry.compactActivityTopPadding)
        .buttonStyle(.plain)
        // A limit's share coming or going widens the wing, as a count does.
        .onChange(of: NotchClaudeSessionsSupport.stripText(counts, limit: limitShare)) { _, _ in
            DispatchQueue.main.async { service.refreshPresentation() }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(FeatureStrings.notchClaudeSessions(l10n.language).title)
        .accessibilityValue(NotchClaudeSessionFormat.counts(counts, text: FeatureStrings.notchClaudeSessions(l10n.language),
                                                            limit: NotchSessionLimitsSupport.currentStripLimit()))
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { open() }
    }

    private func open() {
        // A waiting session opens on its own page, where it can be answered.
        let waiting = sessions.sessions.first { $0.phase.isWaiting }
        service.openActivity(.sessions)
        if let waiting, service.selected == .sessions { sessions.focus(waiting.id) }
    }
}

/// The most pressing phase: a waiting hand, the marks of the agents at
/// work, or a finished check. While Claude and Codex both work their marks
/// stack, Claude's in front on a disc of the strip's black and Codex's
/// cloud showing past it; the counts beside them tell the two apart.
struct NotchClaudeSessionStripMark: View {
    let counts: ClaudeSessionCounts
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let agents = NotchClaudeSessionsSupport.stripAgents(counts)
        Group {
            if counts.waiting > 0 {
                Image(systemName: NotchClaudeSessionsSupport.symbol(.waitingPermission))
                    .font(.system(size: size, weight: .semibold))
                    .foregroundStyle(ClaudeSessionPhase.waitingPermission.tint)
            } else if let front = agents.first {
                stack(front: front, behind: agents.dropFirst().first)
            } else if counts.limited > 0 {
                // Nothing works and the plan stopped something: that comes before a result.
                Image(systemName: NotchClaudeSessionsSupport.symbol(.limited))
                    .font(.system(size: size, weight: .semibold))
                    .foregroundStyle(ClaudeSessionPhase.limited.tint)
            } else {
                Image(systemName: NotchClaudeSessionsSupport.symbol(.finished))
                    .font(.system(size: size, weight: .semibold))
                    .foregroundStyle(ClaudeSessionPhase.finished.tint)
            }
        }
        .accessibilityHidden(true)
    }

    private func stack(front: ClaudeSessionProvider, behind: ClaudeSessionProvider?) -> some View {
        let offset = NotchClaudeSessionsSupport.stripStackOffset(size: size)
        return ZStack(alignment: .leading) {
            if let behind {
                NotchSessionAgentGlyph(provider: behind, size: size)
                    .offset(x: offset)
                    .transition(.asymmetric(insertion: .offset(x: -offset).combined(with: .opacity),
                                            removal: .opacity))
            }
            NotchSessionAgentGlyph(provider: front, size: size)
                .background {
                    if behind != nil {
                        Circle().fill(.black)
                            .frame(width: size * 1.1, height: size * 1.1)
                            .transition(.opacity)
                    }
                }
        }
        .frame(width: NotchClaudeSessionsSupport.stripMarksWidth(counts, size: size), alignment: .leading)
        .animation(reduceMotion ? nil : .spring(response: 0.38, dampingFraction: 0.78), value: behind)
    }
}

/// The counts, each by its phase's symbol: an orange sparkle for Claude's
/// sessions at work, a periwinkle one for Codex's, a coral gauge for the ones
/// a plan limit stopped, and a green sparkle for finished sessions to
/// distinguish the count from the leading check. A plan limit past the
/// warning threshold follows as a ring and its share.
struct NotchClaudeSessionStripCounts: View {
    let counts: ClaudeSessionCounts
    let size: CGFloat
    @ObservedObject private var usage = AgentUsageService.shared
    @ObservedObject private var sessions = ClaudeSessionService.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let limit = NotchSessionLimitsSupport.currentStripLimit()
        HStack(spacing: 6) {
            ForEach(NotchClaudeSessionsSupport.stripReading(counts)) { item in
                HStack(spacing: 2) {
                    Image(systemName: item.phase == .finished ? "sparkle" : NotchClaudeSessionsSupport.symbol(item.phase))
                        .font(.system(size: size * 0.7, weight: .bold))
                    Text("\(item.count)")
                        .font(.system(size: size, weight: .semibold))
                        .monospacedDigit()
                }
                .foregroundStyle(item.tint)
            }
            if let limit {
                let tint = agentLimitTint(limit.provider, usedFraction: limit.window.usedFraction)
                HStack(spacing: 2) {
                    NotchAgentRing(value: NotchAgentSupport.limitDisplay() == .used ? limit.window.usedFraction
                                    : limit.window.remainingFraction, tint: tint, lineWidth: 2)
                        .frame(width: NotchClaudeSessionsSupport.stripLimitRing(textSize: size),
                               height: NotchClaudeSessionsSupport.stripLimitRing(textSize: size))
                    Text(limit.share)
                        .font(.system(size: size * 0.85, weight: .semibold))
                        .monospacedDigit()
                        .contentTransition(.numericText())
                }
                .foregroundStyle(tint)
                .transition(reduceMotion ? .opacity : .scale(scale: 0.6).combined(with: .opacity))
            }
        }
        .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.8), value: limit?.share)
        .lineLimit(1)
        .fixedSize()
        .accessibilityHidden(true)
    }
}

extension NotchSessionLimitsSupport {
    /// The limit the Sessions strip shows now, with its share as written:
    /// among the agents of the sessions listed, as the AI strip's settings
    /// choose it, past the warning threshold.
    static func currentStripLimit(now: Date = Date())
        -> (provider: AgentProvider, window: AgentLimitWindow, share: String)? {
        guard isEnabled() else { return nil }
        let agents = Set(ClaudeSessionService.shared.sessions.map(\.provider.agent))
        let threshold = NotchAgentSupport.limitThreshold() ?? NotchAgentSupport.defaultLimitThreshold
        guard let limit = stripLimit(AgentUsageService.shared.snapshot, agents: agents, focus: NotchAgentSupport.limitFocus(),
                                     threshold: threshold, now: now) else { return nil }
        return (limit.provider, limit.window,
                stripShare(limit.window, display: NotchAgentSupport.limitDisplay(), locale: L10n.shared.language.formattingLocale()))
    }
}
