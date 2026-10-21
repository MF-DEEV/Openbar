// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The Sessions page: every running Claude Code session, the ones waiting on
/// the person first, and one session's page when a row is chosen.
struct NotchClaudeSessionsView: View {
    let size: CGSize
    @ObservedObject private var service = ClaudeSessionService.shared
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.notchSettingsPreview) private var preview
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }

    private var focused: ClaudeSession? {
        guard !preview, let id = service.focusedID else { return nil }
        return service.sessions.first { $0.id == id }
    }

    var body: some View {
        Group {
            if let focused {
                NotchClaudeSessionDetailView(session: focused, size: size)
                    .transition(reduceMotion ? .opacity : .move(edge: .trailing).combined(with: .opacity))
            } else if service.sessions.isEmpty {
                NotchEmptyView(symbol: NotchModule.sessions.symbol, message: text.emptyTitle + "\n" + text.emptyDetail)
            } else {
                list.transition(reduceMotion ? .opacity : .move(edge: .leading).combined(with: .opacity))
            }
        }
        .animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.86), value: focused?.id)
        .frame(width: size.width, height: size.height, alignment: .top)
        .environment(\.locale, l10n.language.formattingLocale())
        .onChange(of: focused?.id, initial: true) { _, id in
            guard !preview else { return }
            // Escape steps back to the list before it closes the island.
            NotchService.shared.setPageLayer(.sessions, close: id == nil ? nil : { service.focus(nil) })
        }
        .onDisappear {
            guard !preview else { return }
            NotchService.shared.setPageLayer(.sessions, close: nil)
            // The island opens on the list again; a notice or the strip
            // choose a session after the page appears.
            service.focus(nil)
        }
    }

    private var list: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            ScrollView {
                LazyVStack(spacing: 6) {
                    ForEach(service.sessions) { session in
                        NotchClaudeSessionRow(session: session, now: context.date, text: text) {
                            guard !preview else { return }
                            service.focus(session.id)
                        }
                    }
                }
                .padding(.bottom, 4)
                .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.85),
                           value: service.sessions.map(\.id))
            }
            .scrollIndicators(.automatic)
        }
        .notchBand(leading: { NotchClaudeSessionCounts(counts: service.counts, total: service.sessions.count) })
    }
}

/// How many sessions work, wait and have news, beside the camera.
private struct NotchClaudeSessionCounts: View {
    let counts: ClaudeSessionCounts
    let total: Int

    var body: some View {
        HStack(spacing: 8) {
            ForEach(NotchClaudeSessionsSupport.stripReading(counts), id: \.phase) { item in
                HStack(spacing: 3) {
                    Image(systemName: NotchClaudeSessionsSupport.symbol(item.phase))
                        .font(.system(size: 9, weight: .semibold))
                    Text("\(item.count)").font(.system(size: 11, weight: .semibold)).monospacedDigit()
                }
                .foregroundStyle(item.phase.tint)
            }
            if counts.isEmpty {
                Text("\(total)").font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 7)
        .frame(height: 28)
        .accessibilityElement(children: .combine)
    }
}

/// One session in the list: where it works, what it does and for how long.
struct NotchClaudeSessionRow: View {
    let session: ClaudeSession
    let now: Date
    let text: NotchClaudeSessionStrings
    let open: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Button(action: open) {
                HStack(spacing: 10) {
                    NotchClaudeSessionBadge(phase: session.phase)
                    summary
                    if !quickDecision {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 9, weight: .semibold))
                            .foregroundStyle(.white.opacity(0.3))
                            .accessibilityHidden(true)
                    }
                }
                .padding(.leading, 11)
                .padding(.trailing, quickDecision ? 0 : 11)
                .frame(maxWidth: .infinity, minHeight: 46, maxHeight: 46)
                .contentShape(Rectangle())
            }
            .buttonStyle(NotchButtonStyle(cornerRadius: 14, lifts: false))
            .accessibilityLabel("\(session.project), \(text.phase(session.phase))")
            .accessibilityValue(NotchClaudeSessionFormat.activity(session, text: text))
            if quickDecision, let permission = session.permission {
                // A request short enough to read whole on the line beside is
                // answered from the list too; a longer one opens its page.
                HStack(spacing: 5) {
                    NotchClaudeDecisionButton(title: text.deny, symbol: "xmark", role: .deny, compact: true) {
                        ClaudeSessionService.shared.decide(session.id, request: permission.requestID, .deny)
                    }
                    NotchClaudeDecisionButton(title: text.allow, symbol: "checkmark", role: .allow, compact: true) {
                        ClaudeSessionService.shared.decide(session.id, request: permission.requestID, .allow)
                    }
                }
                .padding(.trailing, 9)
            }
        }
        .modifier(NotchControlSurface(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .strokeBorder(session.phase.tint.opacity(session.phase.isWaiting ? 0.55 : 0), lineWidth: 1)
                .allowsHitTesting(false)
        }
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private var quickDecision: Bool { session.permission?.fitsOnALine == true }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 5) {
                Text(session.project)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.95))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let branch = session.branch {
                    Label(branch, systemImage: "arrow.triangle.branch")
                        .labelStyle(NotchClaudeSessionInlineLabel())
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 4)
                if let clock = NotchClaudeSessionFormat.clock(session, now: now) {
                    Text(clock)
                        .font(.system(size: 11, weight: .medium))
                        .monospacedDigit()
                        .foregroundStyle(session.phase.tint)
                }
            }
            Text(NotchClaudeSessionFormat.activity(session, text: text))
                .font(.system(size: 10.5))
                .foregroundStyle(session.phase.isWaiting ? session.phase.tint : .secondary)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }
}

/// The phase as a shape and a color, never a color alone.
struct NotchClaudeSessionBadge: View {
    let phase: ClaudeSessionPhase
    var size: CGFloat = 26

    var body: some View {
        ZStack {
            Circle().fill(phase.tint.opacity(0.16))
            if phase.isWorking {
                NotchAgentGlyph(provider: .claude, size: size * 0.42, working: true)
            } else {
                Image(systemName: NotchClaudeSessionsSupport.symbol(phase))
                    .font(.system(size: size * 0.42, weight: .semibold))
                    .foregroundStyle(phase.tint)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

struct NotchClaudeSessionInlineLabel: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 2) {
            configuration.icon.imageScale(.small)
            configuration.title
        }
    }
}

extension ClaudeSessionPhase {
    /// Claude's own color while it works, amber while it waits on the
    /// person, green for a result to read.
    var tint: Color {
        switch self {
        case .generating, .runningTool: return AgentProvider.claude.tint
        case .waitingPermission, .waitingInput: return Color(red: 1.0, green: 0.78, blue: 0.28)
        case .finished: return Color(red: 0.36, green: 0.84, blue: 0.52)
        case .idle: return .white.opacity(0.45)
        }
    }
}

enum NotchClaudeSessionFormat {
    /// The time the turn under way has run, or how long the session has
    /// waited; nothing for a quiet one.
    static func clock(_ session: ClaudeSession, now: Date) -> String? {
        switch session.phase {
        case .generating, .runningTool:
            return AgentFormat.clock(now.timeIntervalSince(session.turnStarted ?? session.phaseSince))
        case .waitingPermission, .waitingInput:
            return AgentFormat.clock(now.timeIntervalSince(session.phaseSince))
        case .finished, .idle:
            return nil
        }
    }

    /// The counts, read aloud.
    static func counts(_ counts: ClaudeSessionCounts, text: NotchClaudeSessionStrings) -> String {
        NotchClaudeSessionsSupport.stripReading(counts).map { "\($0.count) \(text.phase($0.phase))" }.joined(separator: ", ")
    }

    /// What the row's second line says.
    static func activity(_ session: ClaudeSession, text: NotchClaudeSessionStrings) -> String {
        var parts = [text.phase(session.phase)]
        switch session.phase {
        case .waitingPermission:
            if let tool = session.permission?.tool ?? session.currentTool?.name { parts.append(tool) }
            if let permission = session.permission {
                parts.append(permission.fitsOnALine ? permission.detail : permission.summary ?? "")
            }
        case .runningTool:
            if let tool = session.currentTool {
                parts.append(tool.summary.map { "\(tool.name): \($0)" } ?? tool.name)
            }
        case .waitingInput, .generating:
            if let title = session.title { parts.append(title) }
        case .finished, .idle:
            if let title = session.title { parts.append(title) }
            if session.cost > 0 { parts.append(AgentFormat.cost(session.cost)) }
        }
        if let tasks = session.tasks, session.phase.isWorking { parts.append("\(tasks.done)/\(tasks.total)") }
        return parts.joined(separator: " · ")
    }
}
