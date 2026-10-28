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
    @AppStorage(DefaultsKey.notchSessionsBuddies) private var buddies = true

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }

    private var focused: ClaudeSession? {
        guard !preview, let id = service.focusedID else { return nil }
        return service.sessions.first { $0.id == id }
    }

    /// The session whose Clawd is being dressed, over its own page.
    private var dressing: ClaudeSession? {
        guard buddies, let focused, service.dressingID == focused.id else { return nil }
        return focused
    }

    var body: some View {
        Group {
            if let dressing {
                NotchClawdWardrobeView(session: dressing, size: size)
                    .transition(reduceMotion ? .opacity : .move(edge: .trailing).combined(with: .opacity))
            } else if let focused {
                NotchClaudeSessionDetailView(session: focused, size: size)
                    .transition(reduceMotion ? .opacity : .move(edge: .trailing).combined(with: .opacity))
            } else if service.sessions.isEmpty {
                NotchEmptyView(symbol: NotchModule.sessions.symbol, message: text.emptyTitle + "\n" + text.emptyDetail)
            } else {
                list.transition(reduceMotion ? .opacity : .move(edge: .leading).combined(with: .opacity))
            }
        }
        .animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.86), value: focused?.id)
        .animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.86), value: dressing?.id)
        .frame(width: size.width, height: size.height, alignment: .top)
        .environment(\.locale, l10n.language.formattingLocale())
        .onChange(of: [focused?.id, dressing?.id], initial: true) { _, _ in
            guard !preview else { return }
            // Escape steps back from the wardrobe to the page, and from the
            // page to the list, before it closes the island.
            let close: (() -> Void)?
            switch NotchClaudeSessionsSupport.pageLayer(focused: focused?.id, dressing: dressing?.id) {
            case .wardrobe: close = { service.dress(nil) }
            case .session: close = { service.focus(nil) }
            case nil: close = nil
            }
            NotchService.shared.setPageLayer(.sessions, close: close)
        }
        // Clawds turned off take their wardrobe with them, and ⌘Y, ⌘N and
        // ⌘T answer the page again.
        .onChange(of: buddies) { _, shown in
            if !shown { service.dress(nil) }
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
                        NotchClaudeSessionRow(session: session, name: service.displayName(session), now: context.date, text: text) {
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

/// How many sessions work, wait and have news, beside the camera: the ones
/// at work counted by agent, each in its color, as on the closed strip.
private struct NotchClaudeSessionCounts: View {
    let counts: ClaudeSessionCounts
    let total: Int

    var body: some View {
        HStack(spacing: 8) {
            ForEach(NotchClaudeSessionsSupport.stripReading(counts)) { item in
                HStack(spacing: 3) {
                    Image(systemName: NotchClaudeSessionsSupport.symbol(item.phase))
                        .font(.system(size: 9, weight: .semibold))
                    Text("\(item.count)").font(.system(size: 11, weight: .semibold)).monospacedDigit()
                }
                .foregroundStyle(item.tint)
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

/// One session in the list: what it is called, what it does, where it
/// works and for how long.
struct NotchClaudeSessionRow: View {
    let session: ClaudeSession
    let name: String
    let now: Date
    let text: NotchClaudeSessionStrings
    let open: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorSchemeContrast) private var contrast
    @AppStorage(DefaultsKey.notchSessionsBuddies) private var buddies = true
    @State private var hovered = false
    @FocusState private var focused: Bool

    /// A session whose Clawd sleeps fades back a little, and comes forward
    /// under the pointer or the keyboard to be read. With more contrast asked
    /// for, nothing fades.
    private var dimmed: Bool {
        buddies && contrast != .increased && !hovered && !focused && NotchClaudeSessionsSupport.dozes(session, now: now)
    }

    var body: some View {
        HStack(spacing: 6) {
            Button(action: open) {
                HStack(spacing: 10) {
                    NotchClaudeSessionAvatar(session: session, now: now)
                    summary
                    if !quickDecision {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 9, weight: .semibold))
                            .foregroundStyle(.white.opacity(0.3))
                            .accessibilityHidden(true)
                    }
                }
                .opacity(dimmed ? NotchClaudeSessionsSupport.dozingOpacity : 1)
                .animation(reduceMotion ? nil : .easeOut(duration: 0.2), value: dimmed)
                .padding(.leading, 11)
                .padding(.trailing, quickDecision ? 0 : 11)
                .frame(maxWidth: .infinity, minHeight: 46, maxHeight: 46)
                .contentShape(Rectangle())
            }
            .buttonStyle(NotchButtonStyle(cornerRadius: 14, lifts: false))
            .focused($focused)
            .accessibilityLabel("\(name), \(text.agent(session.provider)), \(session.project), \(text.phase(session.phase))")
            .accessibilityValue(NotchClaudeSessionFormat.activity(session, text: text, title: session.title != name))
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
                .strokeBorder(session.tint.opacity(session.phase.isWaiting ? 0.55 : 0), lineWidth: 1)
                .allowsHitTesting(false)
        }
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .onHover { hovered = $0 }
    }

    private var quickDecision: Bool { session.permission?.fitsOnALine == true }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 5) {
                Text(name)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.95))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 4)
                if let clock = NotchClaudeSessionFormat.clock(session, now: now) {
                    Text(clock)
                        .font(.system(size: 11, weight: .medium))
                        .monospacedDigit()
                        .foregroundStyle(session.tint)
                }
            }
            HStack(spacing: 6) {
                // The project first, in its own color, so sessions of one
                // project are found together at a glance.
                NotchClaudeProjectTag(project: session.project)
                    .layoutPriority(2)
                Text(NotchClaudeSessionFormat.activity(session, text: text, title: session.title != name))
                    .foregroundStyle(session.phase.isWaiting ? session.tint : .secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .layoutPriority(1)
                if let branch = session.branch {
                    Label(branch, systemImage: "arrow.triangle.branch")
                        .labelStyle(NotchClaudeSessionInlineLabel())
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .font(.system(size: 10.5))
        }
    }
}

/// A project's name in its color, the same wherever the project shows.
struct NotchClaudeProjectTag: View {
    let project: String
    var size: CGFloat = 10.5

    /// Hues clear of the phases' orange, amber and green.
    static let palette: [Color] = [
        Color(red: 0.42, green: 0.64, blue: 1.0), Color(red: 0.72, green: 0.55, blue: 1.0),
        Color(red: 1.0, green: 0.50, blue: 0.74), Color(red: 0.27, green: 0.82, blue: 0.84),
        Color(red: 0.62, green: 0.84, blue: 0.36), Color(red: 0.84, green: 0.68, blue: 0.52),
        Color(red: 0.38, green: 0.86, blue: 1.0), Color(red: 0.86, green: 0.78, blue: 1.0),
    ]

    static func color(_ project: String) -> Color { palette[NotchClaudeSessionsSupport.projectColor(project) % palette.count] }

    var body: some View {
        let color = Self.color(project)
        Text(project)
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(color)
            .lineLimit(1)
            .truncationMode(.middle)
            .padding(.horizontal, 6)
            .padding(.vertical, 1.5)
            .background(color.opacity(0.16), in: Capsule())
            .frame(maxWidth: 160, alignment: .leading)
            .fixedSize(horizontal: true, vertical: false)
            .accessibilityLabel(project)
    }
}

/// The phase as a shape and a color, never a color alone. At work it shows
/// the mark of the agent doing the work, in that agent's color.
struct NotchClaudeSessionBadge: View {
    let phase: ClaudeSessionPhase
    var provider = ClaudeSessionProvider.claude
    var size: CGFloat = 26

    var body: some View {
        let tint = phase.tint(for: provider)
        ZStack {
            Circle().fill(tint.opacity(0.16))
            if phase.isWorking {
                NotchSessionAgentGlyph(provider: provider, size: size * 0.42, working: true)
            } else {
                Image(systemName: NotchClaudeSessionsSupport.symbol(phase))
                    .font(.system(size: size * 0.42, weight: .semibold))
                    .foregroundStyle(tint)
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
    /// The phase's color in a Claude Code session, and for the phases every
    /// agent shares.
    var tint: Color { tint(for: .claude) }

    /// The agent's own color while it works, amber while it waits on the
    /// person, green for a result to read: only work tells agents apart.
    func tint(for provider: ClaudeSessionProvider) -> Color {
        switch self {
        case .generating, .runningTool: return provider.tint
        case .background: return provider.tint.opacity(0.75)
        case .waitingPermission, .waitingInput: return Color(red: 1.0, green: 0.78, blue: 0.28)
        case .finished: return Color(red: 0.36, green: 0.84, blue: 0.52)
        case .idle: return .white.opacity(0.45)
        }
    }
}

extension ClaudeSessionProvider {
    /// The agent behind a session, as the AI section knows it.
    var agent: AgentProvider {
        switch self {
        case .claude: return .claude
        case .codex: return .codex
        }
    }

    /// Claude's orange or Codex's periwinkle, the same as on the AI page.
    var tint: Color { agent.tint }
}

extension ClaudeSession {
    /// The session's phase in its agent's color.
    var tint: Color { phase.tint(for: provider) }
}

extension NotchClaudeSessionsSupport.StripItem {
    /// An agent's count at work in its color; the rest in the phase's.
    var tint: Color { phase.tint(for: provider ?? .claude) }
}

enum NotchClaudeSessionFormat {
    /// Links read as links: blue, in Markdown or written out.
    static let linkColor = Color(red: 0.42, green: 0.66, blue: 1.0)

    static func markdown(_ text: String) -> AttributedString {
        var result = NotchClaudeSessionsSupport.markdown(text)
        for run in result.runs where run.link != nil { result[run.range].foregroundColor = linkColor }
        return result
    }

    /// The time the turn under way has run, or how long the session has
    /// waited; nothing for a quiet one.
    static func clock(_ session: ClaudeSession, now: Date) -> String? {
        switch session.phase {
        case .generating, .runningTool:
            return AgentFormat.clock(now.timeIntervalSince(session.turnStarted ?? session.phaseSince))
        case .waitingPermission, .waitingInput, .background:
            return AgentFormat.clock(now.timeIntervalSince(session.phaseSince))
        case .finished, .idle:
            return nil
        }
    }

    /// The counts, read aloud, the ones at work with whose they are.
    static func counts(_ counts: ClaudeSessionCounts, text: NotchClaudeSessionStrings) -> String {
        NotchClaudeSessionsSupport.stripReading(counts).map { item in
            let reading = "\(item.count) \(text.phase(item.phase))"
            return item.provider.map { "\(reading) (\(text.agent($0)))" } ?? reading
        }.joined(separator: ", ")
    }

    /// Why a reply did not go.
    static func problem(_ failure: ClaudeSessionService.SendFailure, text: NotchClaudeSessionStrings) -> String {
        switch failure {
        case .waiting: return text.sendWhileWaiting
        case .unreachable: return text.sendFailed
        }
    }

    /// What the row's second line says: what the session does now. The
    /// title is left out where the session
    /// is already called by it.
    static func activity(_ session: ClaudeSession, text: NotchClaudeSessionStrings, title showsTitle: Bool = true) -> String {
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
        case .waitingInput:
            if let permission = session.permission, permission.asksQuestions {
                parts.append(permission.summary ?? "")
            } else if showsTitle, let title = session.title {
                parts.append(title)
            }
        case .generating:
            if showsTitle, let title = session.title { parts.append(title) }
        case .background:
            if let label = session.backgroundLabel { parts.append(label) } else if showsTitle, let title = session.title { parts.append(title) }
            if session.cost > 0 { parts.append(AgentFormat.cost(session.cost)) }
        case .finished, .idle:
            if showsTitle, let title = session.title { parts.append(title) }
            if session.cost > 0 { parts.append(AgentFormat.cost(session.cost)) }
        }
        // A dev server or an emulator left running, which is not work.
        if session.services > 0, !session.phase.isWorking { parts.append(text.leftRunning(session.services)) }
        if let tasks = session.tasks, session.phase.isWorking { parts.append("\(tasks.done)/\(tasks.total)") }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}
