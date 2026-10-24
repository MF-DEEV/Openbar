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
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        NotchStripHold(sessions.counts, shows: !sessions.counts.isEmpty) { strip(counts: $0) }
    }

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
        .onChange(of: NotchClaudeSessionsSupport.stripText(counts)) { _, _ in
            DispatchQueue.main.async { service.refreshPresentation() }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(FeatureStrings.notchClaudeSessions(l10n.language).title)
        .accessibilityValue(NotchClaudeSessionFormat.counts(counts, text: FeatureStrings.notchClaudeSessions(l10n.language)))
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

/// The most pressing phase: a waiting hand, the mark of the agent at work,
/// or a finished check. Claude's mark shows while any Claude session works,
/// Codex's when only Codex does; the counts beside it tell the two apart, so
/// the mark shows only once.
struct NotchClaudeSessionStripMark: View {
    let counts: ClaudeSessionCounts
    let size: CGFloat

    var body: some View {
        Group {
            if counts.waiting > 0 {
                Image(systemName: NotchClaudeSessionsSupport.symbol(.waitingPermission))
                    .font(.system(size: size, weight: .semibold))
                    .foregroundStyle(ClaudeSessionPhase.waitingPermission.tint)
            } else if let agent = NotchClaudeSessionsSupport.stripAgent(counts) {
                NotchSessionAgentGlyph(provider: agent, size: size)
            } else {
                Image(systemName: NotchClaudeSessionsSupport.symbol(.finished))
                    .font(.system(size: size, weight: .semibold))
                    .foregroundStyle(ClaudeSessionPhase.finished.tint)
            }
        }
        .accessibilityHidden(true)
    }
}

/// The counts, each by its phase's symbol: an orange sparkle for Claude's
/// sessions at work and a periwinkle one for Codex's, never a logo.
struct NotchClaudeSessionStripCounts: View {
    let counts: ClaudeSessionCounts
    let size: CGFloat

    var body: some View {
        HStack(spacing: 6) {
            ForEach(NotchClaudeSessionsSupport.stripReading(counts)) { item in
                HStack(spacing: 2) {
                    Image(systemName: NotchClaudeSessionsSupport.symbol(item.phase))
                        .font(.system(size: size * 0.7, weight: .bold))
                    Text("\(item.count)")
                        .font(.system(size: size, weight: .semibold))
                        .monospacedDigit()
                }
                .foregroundStyle(item.tint)
            }
        }
        .lineLimit(1)
        .fixedSize()
        .accessibilityHidden(true)
    }
}
