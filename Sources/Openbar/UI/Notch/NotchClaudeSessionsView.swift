// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The Sessions page: every running Claude Code session, the ones waiting on
/// the person first, and one session's page when a row is chosen.
struct NotchClaudeSessionsView: View {
    let size: CGSize
    @ObservedObject private var service = ClaudeSessionService.shared
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var away = NotchAwayCard.shared
    @Environment(\.notchSettingsPreview) private var preview
    @Environment(\.notchSessionsInPanel) private var inPanel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage(DefaultsKey.notchSessionsBuddies) private var buddies = true
    @AppStorage(DefaultsKey.notchSessionsGrouping) private var groupingRaw = NotchSessionGrouping.none.rawValue
    @AppStorage(DefaultsKey.notchSessionsCollapsedGroups) private var collapsedRaw = ""

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }
    private var grouping: NotchSessionGrouping { NotchSessionGrouping(rawValue: groupingRaw) ?? .none }

    /// The open page's session, with the ask the queue opened it on (#269).
    private var focused: ClaudeSession? {
        guard !preview else { return nil }
        return service.focusedSession
    }

    private var queue: NotchPermissionQueue { NotchPermissionQueue(service.sessions) }

    /// The session whose Clawd is being dressed, over its own page.
    private var dressing: ClaudeSession? {
        guard buddies, let focused, service.dressingID == focused.id else { return nil }
        return focused
    }

    var body: some View {
        Group {
            if away.isShown, !preview {
                NotchAwayCardView(size: size, text: text)
                    .transition(reduceMotion ? .opacity : .scale(scale: 0.96, anchor: .top).combined(with: .opacity))
            } else if service.composingSession, !preview {
                NotchClaudeNewSessionView(text: text)
                    .transition(reduceMotion ? .opacity : .move(edge: .trailing).combined(with: .opacity))
            } else if let dressing {
                NotchClawdWardrobeView(session: dressing, size: size)
                    .transition(reduceMotion ? .opacity : .move(edge: .trailing).combined(with: .opacity))
            } else if let focused {
                NotchClaudeSessionDetailView(session: focused, size: size)
                    .transition(reduceMotion ? .opacity : .move(edge: .trailing).combined(with: .opacity))
            } else if service.sessions.isEmpty {
                NotchEmptyView(symbol: NotchModule.sessions.symbol, message: text.emptyTitle + "\n" + text.emptyDetail)
                    .notchBand(trailing: { newSessionButton })
            } else {
                list.transition(reduceMotion ? .opacity : .move(edge: .leading).combined(with: .opacity))
            }
        }
        .animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.86), value: focused?.id)
        .animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.86), value: dressing?.id)
        .animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.86), value: service.composingSession)
        .animation(reduceMotion ? nil : .spring(response: 0.36, dampingFraction: 0.82), value: away.isShown)
        .frame(width: size.width, height: size.height, alignment: .top)
        .environment(\.locale, l10n.language.formattingLocale())
        .onChange(of: [focused?.id, dressing?.id, service.composingSession ? "new" : nil], initial: true) { _, _ in
            guard !preview else { return }
            if service.composingSession {
                NotchSessionsHost.setPageLayer(inPanel: inPanel, close: { service.composingSession = false })
                return
            }
            // Escape steps back from the wardrobe to the page, and from the
            // page to the list, before it closes the island.
            let close: (() -> Void)?
            switch NotchClaudeSessionsSupport.pageLayer(focused: focused?.id, dressing: dressing?.id) {
            case .wardrobe: close = { service.dress(nil) }
            case .session: close = { service.focus(nil) }
            case nil: close = nil
            }
            NotchSessionsHost.setPageLayer(inPanel: inPanel, close: close)
        }
        // Clawds turned off take their wardrobe with them, and ⌘Y, ⌘N and
        // ⌘T answer the page again.
        .onChange(of: buddies) { _, shown in
            if !shown { service.dress(nil) }
        }
        .onDisappear {
            guard !preview else { return }
            NotchSessionsHost.setPageLayer(inPanel: inPanel, close: nil)
            // The other place took the sessions over (#331): the island from
            // the menu bar's panel, or the panel from the island. Its page
            // keeps the session and the card open.
            let notch = NotchService.shared
            let handedOver = inPanel ? notch.expanded && notch.selected == .sessions : SessionsMenuBar.shared.panelShown
            guard !handedOver else { return }
            // The card goes with the island: closed, or opened on something else.
            NotchAwayCard.shared.close(collapse: false)
            // The island opens on the list again; a notice or the strip
            // choose a session after the page appears.
            service.focus(nil)
            service.composingSession = false
        }
    }

    private var list: some View {
        let queue = queue
        return TimelineView(.periodic(from: .now, by: 1)) { context in
            ScrollView {
                // More than one ask waiting: all of them first, in one queue (#269).
                if queue.shows, !preview {
                    NotchPermissionQueueView(queue: queue, text: text)
                        .padding(.bottom, 4)
                        .transition(reduceMotion ? .opacity : .opacity.combined(with: .move(edge: .top)))
                }
                if grouping == .none {
                    LazyVStack(spacing: 6) {
                        ForEach(service.sessions) { session in row(session, now: context.date) }
                    }
                    .padding(.bottom, 4)
                    .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.85),
                               value: service.sessions.map(\.id))
                } else {
                    grouped(now: context.date)
                }
            }
            .scrollIndicators(.automatic)
            .animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.86), value: queue.shows)
        }
        // The open island shows every session, so the band leads with how
        // they're laid out rather than counting them again (#233).
        .notchBand(leading: { NotchSessionGroupingSwitch(text: text) },
                   trailing: { newSessionButton })
    }

    private func row(_ session: ClaudeSession, now: Date, inProject: Bool = false) -> some View {
        NotchClaudeSessionRow(session: session, name: service.displayName(session), now: now, text: text,
                              account: service.accountLabels[session.id], inProject: inProject,
                              // The queue above answers it; the row only says it waits.
                              quickDecisions: preview || !queue.shows) {
            guard !preview else { return }
            service.focus(session.id)
        }
    }

    /// The list in groups (#233): groups with someone waiting on top, each
    /// folding its rows away.
    private func grouped(now: Date) -> some View {
        let groups = NotchSessionGroups.groups(service.sessions, by: grouping)
        let collapsed = Set(NotchSessionGroupStore.keys(collapsedRaw))
        let spring: Animation? = reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.85)
        return LazyVStack(alignment: .leading, spacing: 10) {
            ForEach(groups) { group in
                let folded = collapsed.contains(group.key)
                VStack(spacing: 4) {
                    NotchSessionGroupHeader(group: group, collapsed: folded, text: text) {
                        withAnimation(reduceMotion ? .easeOut(duration: 0.12) : .spring(response: 0.34, dampingFraction: 0.86)) {
                            collapsedRaw = NotchSessionGroupStore.raw(
                                NotchSessionGroups.toggled(NotchSessionGroupStore.keys(collapsedRaw), key: group.key))
                        }
                    }
                    if !folded {
                        VStack(spacing: 6) {
                            ForEach(group.sessions) { session in row(session, now: now, inProject: group.kind == .project) }
                        }
                        .transition(reduceMotion ? .opacity
                                    : .asymmetric(insertion: .opacity.combined(with: .move(edge: .top)), removal: .opacity))
                    }
                }
                .clipped()
            }
        }
        .padding(.bottom, 4)
        .animation(spring, value: groups.map(\.key))
        .animation(spring, value: service.sessions.map(\.id))
    }

    /// Starts a session from here, where Claude Code is there to start one.
    @ViewBuilder private var newSessionButton: some View {
        if service.canStartSessions || service.canStartCodex, !preview {
            Button {
                service.composingSession = true
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "plus").font(.system(size: 11, weight: .semibold))
                    Text(text.newSession).font(.system(size: 12, weight: .medium)).lineLimit(1)
                }
                .foregroundStyle(.white.opacity(0.9))
                .padding(.horizontal, 9)
                .frame(height: 28)
                .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            }
            .buttonStyle(NotchButtonStyle(cornerRadius: 9))
            .help(text.newSession)
        }
    }
}

/// One session in the list: what it is called, what it does, where it
/// works and for how long.
struct NotchClaudeSessionRow: View {
    let session: ClaudeSession
    let name: String
    let now: Date
    let text: NotchClaudeSessionStrings
    /// The Claude account it runs under, while more than one is in use.
    var account: String? = nil
    /// In a project's group, whose header names the project (#233).
    var inProject = false
    /// Allow and Deny beside a short ask; off while the queue shows it (#269).
    var quickDecisions = true
    let open: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorSchemeContrast) private var contrast
    @AppStorage(DefaultsKey.notchSessionsBuddies) private var buddies = true
    @AppStorage(DefaultsKey.notchSessionsProgressStyle) private var progressRaw = NotchSessionProgressStyle.ring.rawValue
    @State private var hovered = false
    @FocusState private var focused: Bool

    private var progressStyle: NotchSessionProgressStyle { NotchSessionProgressStyle(rawValue: progressRaw) ?? .ring }
    /// The session's task list, when it keeps one (#215).
    private var tasks: ClaudeTaskProgress? { session.tasks.flatMap { $0.total > 0 ? $0 : nil } }

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
                    NotchClaudeSessionAvatar(session: session, now: now, agentMark: true)
                        .overlay {
                            if let tasks, progressStyle == .ring {
                                NotchTaskRing(tasks: tasks, tint: session.provider.tint, gapped: true)
                                    .frame(width: 42, height: 42)
                            }
                        }
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
            .accessibilityLabel([name, text.agent(session.provider), session.project,
                                 NotchSessionGroups.branchLabel(session)?.full, text.phase(session.phase)]
                .compactMap { $0 }.joined(separator: ", "))
            .accessibilityValue(NotchClaudeSessionFormat.activity(session, text: text, title: session.title != name, now: now))
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
        .overlay(alignment: .bottom) {
            if let tasks, progressStyle == .bar { NotchTaskBar(tasks: tasks, tint: session.provider.tint) }
        }
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .strokeBorder(session.tint.opacity(session.phase.isWaiting ? 0.55 : session.phase == .limited ? 0.4 : 0),
                              lineWidth: 1)
                .allowsHitTesting(false)
        }
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .onHover { hovered = $0 }
    }

    private var quickDecision: Bool { quickDecisions && session.permission?.fitsOnALine == true }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 5) {
                Text(name)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.95))
                    .lineLimit(1)
                    .truncationMode(.tail)
                if session.background != nil { NotchClaudeBackgroundMark(text: text) }
                if let alias = session.host.remoteAlias { NotchRemoteHostBadge(alias: alias) }
                if let origin = session.origin { NotchClaudeOriginBadge(origin: origin, text: text) }
                if let multiplexer = session.host.multiplexer { NotchMultiplexerBadge(name: multiplexer, text: text) }
                if let editor = session.editor { NotchEditorBadge(agent: session.provider.displayName, editor: editor, text: text) }
                if let account { NotchClaudeAccountBadge(name: account) }
                if session.autoMode { NotchSessionAutoBadge(provider: session.provider, text: text) }
                Spacer(minLength: 4)
                NotchModelChip(session: session)
                if let tasks { NotchTaskCount(tasks: tasks, tint: session.provider.tint, style: progressStyle, text: text) }
                if let clock = NotchClaudeSessionFormat.clock(session, now: now) {
                    Text(clock)
                        .font(.system(size: 11, weight: .medium))
                        .monospacedDigit()
                        .foregroundStyle(session.tint)
                }
            }
            HStack(spacing: 6) {
                // The project first, in its own color, so sessions of one
                // project are found together at a glance; in a project's
                // group the header names it.
                if !inProject {
                    NotchClaudeProjectTag(project: session.project)
                        .layoutPriority(3)
                }
                // Then where it works: the branch, and a worktree's name (#233).
                NotchSessionBranchChip(session: session)
                    .layoutPriority(2)
                Text(NotchClaudeSessionFormat.activity(session, text: text, title: session.title != name, now: now))
                    .foregroundStyle(session.phase.isWaiting || session.phase == .limited ? session.tint : .secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .layoutPriority(1)
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
        // Coral, apart from waiting's amber: the work stopped and waits on the plan.
        case .limited: return Color(red: 1.0, green: 0.46, blue: 0.42)
        case .finished: return Color(red: 0.36, green: 0.84, blue: 0.52)
        case .idle: return .white.opacity(0.45)
        }
    }
}

extension ClaudeSessionProvider {
    /// Claude's orange or Codex's periwinkle, the same as on the AI page,
    /// and OpenCode's and Copilot's from there too. Gemini CLI gets the cyan
    /// of its star's first light, and Cursor a silver for its grey cube. Pi
    /// takes the pink of its P, Mistral Vibe the amber of Mistral's M,
    /// Devin CLI a violet no other agent wears, and Google Antigravity the
    /// deep blue of its arch's foot. Grok CLI, whose mark has no color,
    /// gets the moss green no other agent wears, and ZCode, black and white
    /// too, a magenta no other agent wears. Hermes takes the pale gold of
    /// its terminal's own colors, apart from Vibe's amber, and Qwen Code the
    /// indigo of its mark, between Antigravity's blue and Devin's violet.
    /// Factory Droid, white on black too, the bright tangerine of its
    /// terminal's own highlights, redder than Vibe's amber and brighter
    /// than Claude's clay. Cline, white on black as well, a sea green no
    /// other agent wears, between Gemini's sky and Grok's moss. Kimi Code
    /// the pale blue its mark's head fades to, lighter than Antigravity's
    /// and less green than Gemini's sky. Kiro CLI the violet of its ghost's
    /// ground, deeper than Devin's orchid and redder than Qwen's indigo.
    var tint: Color {
        if let agent { return agent.tint }
        switch self {
        case .gemini: return Color(red: 0.30, green: 0.74, blue: 0.98)
        case .pi: return Color(red: 0.95, green: 0.55, blue: 0.56)
        case .vibe: return Color(red: 1.0, green: 0.69, blue: 0.0)
        case .devin: return Color(red: 0.70, green: 0.55, blue: 1.0)
        case .antigravity: return Color(red: 0.19, green: 0.53, blue: 1.0)
        case .grok: return Color(red: 0.55, green: 0.80, blue: 0.40)
        case .zcode: return Color(red: 0.88, green: 0.44, blue: 0.78)
        case .hermes: return Color(red: 0.96, green: 0.84, blue: 0.40)
        case .qwen: return Color(red: 0.47, green: 0.45, blue: 1.0)
        case .droid: return Color(red: 1.0, green: 0.42, blue: 0.13)
        case .cline: return Color(red: 0.25, green: 0.84, blue: 0.74)
        case .kimi: return Color(red: 0.47, green: 0.71, blue: 1.0)
        case .kiro: return Color(red: 0.60, green: 0.36, blue: 1.0)
        default: return Color(red: 0.80, green: 0.82, blue: 0.90)
        }
    }
}

extension ClaudeSession {
    /// The session's phase in its agent's color.
    var tint: Color { phase.tint(for: provider) }
}

extension NotchClaudeSessionsSupport.StripItem {
    /// An agent's count at work in its color; the rest in the phase's. A
    /// count joining several agents at work takes none of theirs.
    var tint: Color {
        if phase == .generating, provider == nil { return .white.opacity(0.9) }
        return phase.tint(for: provider ?? .claude)
    }
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
        case .waitingPermission, .waitingInput:
            // From a minute on, how long it has waited in words (#231).
            if let minutes = NotchSessionReminders.waitedMinutes(session, now: now) {
                return FeatureStrings.notchClaudeSessions(L10n.shared.language).waitingFor(minutes: minutes)
            }
            return AgentFormat.clock(now.timeIntervalSince(session.phaseSince))
        case .background:
            return AgentFormat.clock(now.timeIntervalSince(session.phaseSince))
        case .limited, .finished, .idle:
            return nil
        }
    }

    /// The counts, read aloud, the ones at work with whose they are, and the
    /// limit the strip shows.
    static func counts(_ counts: ClaudeSessionCounts, text: NotchClaudeSessionStrings,
                       limit: (provider: AgentProvider, window: AgentLimitWindow, share: String)? = nil) -> String {
        let items = NotchClaudeSessionsSupport.stripReading(counts).map { item in
            let reading = "\(item.count) \(text.phase(item.phase))"
            return item.provider.map { "\(reading) (\(text.agent($0)))" } ?? reading
        }
        let agentText = FeatureStrings.notchAgents(L10n.shared.language)
        let reading = limit.map { limit in
            "\(limit.provider.displayName): " + (NotchAgentSupport.limitDisplay() == .used
                ? agentText.usedShare(limit.share) : agentText.left(limit.share))
        }
        return (items + [reading].compactMap { $0 }).joined(separator: ", ")
    }

    /// Why a reply did not go.
    static func problem(_ failure: ClaudeSessionService.SendFailure, text: NotchClaudeSessionStrings) -> String {
        switch failure {
        case .waiting: return text.sendWhileWaiting
        case .unreachable: return text.sendFailed
        }
    }

    /// The phase in words, or what Agent View says of a background session.
    static func phaseName(_ session: ClaudeSession, text: NotchClaudeSessionStrings) -> String {
        NotchClaudeSessionsSupport.backgroundPhase(session, text: text) ?? text.phase(session.phase)
    }

    /// What the row's second line says: what the session does now. The
    /// title is left out where the session
    /// is already called by it.
    static func activity(_ session: ClaudeSession, text: NotchClaudeSessionStrings, title showsTitle: Bool = true,
                         now: Date = Date()) -> String {
        var parts = [phaseName(session, text: text)]
        switch session.phase {
        case .limited:
            // When it goes on: as the agent said, or as the limits reading has it.
            if let resets = NotchSessionLimitsSupport.reset(of: session, snapshot: AgentUsageService.shared.snapshot, now: now,
                                                                accounts: AgentUsageService.shared.claudeAccounts) {
                parts.append(text.resumes(NotchSessionLimitsSupport.resetText(
                    resets, now: now, locale: L10n.shared.language.formattingLocale())))
            }
            if showsTitle, let title = session.title { parts.append(title) }
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
        // The row draws the task list's reading itself (#215).
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}
