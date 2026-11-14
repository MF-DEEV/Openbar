// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI
import AppKit

/// One session's page: what it works on, its latest tools, tasks and
/// subagents, what it said last, and the ways to answer it.
struct NotchClaudeSessionDetailView: View {
    let session: ClaudeSession
    let size: CGSize
    @ObservedObject private var service = ClaudeSessionService.shared
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var notch = NotchService.shared
    @AppStorage(DefaultsKey.notchSessionsContentPreview) private var previews = false
    @State private var openFailed = false
    @State private var renaming = false
    @State private var newName = ""
    @FocusState private var nameFocused: Bool
    /// The recent output of a background session, in place of its work.
    @State private var showingLogs = false

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }
    /// Working subagents come first, then the latest done.
    static let shownSubagents = 4

    var body: some View {
        VStack(spacing: 8) {
            if renaming { nameField }
            if session.background != nil {
                NotchClaudeBackgroundBar(session: session, text: text, showingLogs: $showingLogs)
            }
            if session.worktree != nil {
                NotchClaudeWorktreeBar(session: session, text: text)
            }
            if showingLogs, session.background != nil {
                NotchClaudeBackgroundLogsView(session: session, text: text)
            } else if notch.sessionsEnlarged {
                NotchClaudeSessionLargeView(session: session, text: text, open: open)
            } else {
                VStack(spacing: 8) {
                    work
                    // Claude at work, at the foot of the page as on the enlarged one.
                    if session.phase.isWorking {
                        NotchClaudeThinkingRow(session: session, size: 13)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.leading, 2)
                    }
                    NotchClaudeSessionReplyField(session: session, text: text)
                }
            }
        }
        .notchBand(leading: {
            NotchBandBackButton(title: service.displayName(session)) { service.focus(nil) }
        }, trailing: {
            HStack(spacing: 2) {
                renameButton
                expandButton
                openButton
            }
        })
    }

    private var work: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            ScrollViewReader { page in
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    header(now: context.date)
                    NotchClaudeAutoAllowBar(session: session, text: text)
                    if session.phase.isWaiting {
                        // An opened command brings its buttons into sight.
                        NotchClaudeSessionPermissionCard(session: session, text: text, open: open,
                                                         reveal: { withAnimation(.easeOut(duration: 0.2)) { page.scrollTo(Self.permissionCard, anchor: .bottom) } })
                            .id(Self.permissionCard)
                    }
                    if let title = session.title {
                        Text(title)
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(.white.opacity(0.95))
                            .lineLimit(2)
                    }
                    if let tasks = session.tasks { tasksRow(tasks) }
                    if !session.timeline.isEmpty { timeline }
                    if !session.subagents.isEmpty { subagents(now: context.date) }
                    lastReply
                }
                .padding(.bottom, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollIndicators(.automatic)
            }
        }
    }
    private static let permissionCard = "permission-card"

    // MARK: Header

    private func header(now: Date) -> some View {
        HStack(spacing: 10) {
            NotchClawdDressButton(session: session, name: service.displayName(session), text: text)
                // Room for the tallest hat to jump in at the top of the
                // scrolling page, which clips what reaches past it.
                .padding(.top, 12)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(NotchClaudeSessionFormat.phaseName(session, text: text))
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(session.tint)
                    if let clock = NotchClaudeSessionFormat.clock(session, now: now) {
                        Text(clock).font(.system(size: 11, weight: .medium)).monospacedDigit().foregroundStyle(.secondary)
                    }
                    if let tool = session.currentTool, session.phase == .runningTool {
                        Text(tool.summary.map { "\(tool.name): \($0)" } ?? tool.name)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.white.opacity(0.75))
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                HStack(spacing: 6) {
                    if let alias = session.host.remoteAlias { NotchRemoteHostBadge(alias: alias) }
                    if let account = service.accountLabels[session.id] { NotchClaudeAccountBadge(name: account) }
                    NotchClaudeProjectTag(project: session.project)
                    NotchModelChip(session: session)
                    Text(details)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .accessibilityElement(children: .combine)
            Spacer(minLength: 0)
        }
    }

    private var details: String {
        // The chip beside the tag names the model (#215).
        [session.branch.map { "⎇ \($0)" },
         NotchSessionProgress.shortModel(session) == nil ? session.model.map(AgentPricing.displayName) : nil,
         session.tokens.total > 0 ? AgentFormat.tokens(session.tokens.total) : nil,
         session.cost > 0 ? AgentFormat.cost(session.cost) : nil]
            .compactMap { $0 }.joined(separator: " · ")
    }

    private func open() {
        service.openHost(session) { found in
            openFailed = !found
            if !found {
                DispatchQueue.main.asyncAfter(deadline: .now() + 3) { openFailed = false }
            }
        }
    }

    private var renameButton: some View {
        Button {
            if renaming { saveName() } else {
                newName = service.names[session.id] ?? ""
                renaming = true
                nameFocused = true
            }
        } label: {
            Image(systemName: renaming ? "checkmark" : "pencil")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.white.opacity(0.9))
                .frame(width: 28, height: 28)
                .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 9))
        .help(text.rename)
        .accessibilityLabel(text.rename)
    }

    /// A name of the person's own; left empty, the session's own name returns.
    private var nameField: some View {
        TextField(NotchClaudeSessionsSupport.name(session, custom: nil), text: $newName)
            .textFieldStyle(.plain)
            .font(.system(size: 13, weight: .semibold))
            .focused($nameFocused)
            .onSubmit(saveName)
            .padding(.horizontal, 12)
            .frame(height: 34)
            .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(.white.opacity(0.25), lineWidth: 1)
            }
            .accessibilityLabel(text.renamePlaceholder)
            .onAppear { nameFocused = true }
    }

    private func saveName() {
        service.rename(session.id, to: newName)
        renaming = false
    }

    /// Grows the island around the page, to read and write in comfortably.
    private var expandButton: some View {
        Button {
            notch.setSessionsEnlarged(!notch.sessionsEnlarged)
        } label: {
            Image(systemName: notch.sessionsEnlarged ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.white.opacity(0.9))
                .frame(width: 28, height: 28)
                .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 9))
        .help(notch.sessionsEnlarged ? text.shrink : text.enlarge)
        .accessibilityLabel(notch.sessionsEnlarged ? text.shrink : text.enlarge)
    }

    private var openButton: some View {
        Button(action: open) {
            HStack(spacing: 4) {
                Image(systemName: openFailed ? "exclamationmark.triangle.fill" : "arrow.up.forward.app")
                    .font(.system(size: 11, weight: .semibold))
                Text(openFailed ? text.openFailed : text.open)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
            }
            .foregroundStyle(.white.opacity(openFailed ? 0.6 : 0.9))
            .padding(.horizontal, 9)
            .frame(height: 28)
            .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 9))
        .disabled(session.host == .unknown)
        .help(session.host.displayName.map(text.openIn) ?? text.open)
    }

    // MARK: Work

    private func tasksRow(_ tasks: ClaudeTaskProgress) -> some View {
        NotchTaskChecklist(tasks: tasks, tint: session.provider.tint, text: text)
    }

    private var timeline: some View {
        VStack(alignment: .leading, spacing: 4) {
            sectionTitle(text.recentTools)
            ScrollViewReader { reader in
                ScrollView(.horizontal) {
                    HStack(spacing: 5) {
                        ForEach(session.timeline) { tool in
                            toolChip(tool).id(tool.id)
                        }
                    }
                }
                .scrollIndicators(.hidden)
                .onAppear { reader.scrollTo(session.timeline.last?.id, anchor: .trailing) }
                .onChange(of: session.timeline.last?.id) { _, id in reader.scrollTo(id, anchor: .trailing) }
            }
        }
    }

    private func toolChip(_ tool: ClaudeToolUse) -> some View {
        HStack(spacing: 4) {
            Image(systemName: tool.isRunning ? "circle.dotted" : (tool.failed ? "xmark.circle.fill" : "checkmark.circle.fill"))
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(tool.isRunning ? session.provider.tint : (tool.failed ? .red : .white.opacity(0.45)))
            Text(tool.summary.map { "\(tool.name) \($0)" } ?? tool.name)
                .font(.system(size: 10.5, design: tool.summary == nil ? .default : .monospaced))
                .foregroundStyle(.white.opacity(tool.isRunning ? 0.95 : 0.7))
                .lineLimit(1)
                .frame(maxWidth: 180)
        }
        .padding(.horizontal, 7)
        .frame(height: 22)
        .background(.white.opacity(tool.isRunning ? 0.12 : 0.06), in: Capsule())
        .accessibilityElement(children: .combine)
    }

    private func subagents(now: Date) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            sectionTitle(text.subagents)
            ForEach(session.subagents.prefix(Self.shownSubagents)) { agent in
                HStack(spacing: 6) {
                    NotchClaudeSubagentAvatar(agent: agent, session: session) {
                        Image(systemName: agent.isRunning ? "arrow.triangle.branch" : "checkmark")
                            .font(.system(size: 9, weight: .semibold))
                            .foregroundStyle(agent.isRunning ? session.provider.tint : .white.opacity(0.45))
                            .frame(width: 12)
                    }
                    Text(agent.type).font(.system(size: 11, weight: .medium)).foregroundStyle(.white.opacity(0.85))
                    if let description = agent.description {
                        Text(description).font(.system(size: 10.5)).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer(minLength: 4)
                    Text(AgentFormat.clock((agent.finished ?? now).timeIntervalSince(agent.started)))
                        .font(.system(size: 10)).monospacedDigit().foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            }
            if session.subagents.count > Self.shownSubagents {
                Text(text.moreSubagents(session.subagents.count - Self.shownSubagents))
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder private var lastReply: some View {
        if let reply = session.lastText {
            VStack(alignment: .leading, spacing: 4) {
                sectionTitle(text.lastReply)
                NotchClaudeRichText(text: NotchClaudeSessionFormat.markdown(reply), size: 11,
                                    color: .white.withAlphaComponent(0.8), maxLines: 6)
            }
        } else if !previews {
            VStack(alignment: .leading, spacing: 6) {
                Text(text.previewsOff(for: session.provider)).font(.system(size: 10)).foregroundStyle(.tertiary)
                Button(text.contentPreview) {
                    previews = true
                    service.syncWithPreferences()
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 8))
                .help(text.contentPreviewHint)
            }
        }
    }

    private func sectionTitle(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 9, weight: .semibold))
            .tracking(0.6)
            .foregroundStyle(.white.opacity(0.45))
    }
}

/// What a waiting session asks for. A permission the hooks hold can be
/// answered here; anything else is answered in the session's terminal.
struct NotchClaudeSessionPermissionCard: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    let open: () -> Void
    /// How tall a file change's diff grows before it scrolls: about five
    /// lines on the island, so the buttons stay in sight below it.
    var diffHeight: CGFloat = 76
    /// Scrolls the page so the card's buttons show, once the whole command opens.
    var reveal: (() -> Void)? = nil
    @ObservedObject private var service = ClaudeSessionService.shared
    /// A request that replaced the one on screen holds the buttons back a
    /// moment, so a click meant for the old one cannot answer it.
    @State private var settling = false
    @State private var inspection: PermissionInspection?
    @State private var loadingInspection = false
    @State private var inspectionFailed = false
    @State private var viewerPresented = false
    @State private var inspectionGeneration = UUID()
    private var revision: String? { session.permission.flatMap { p in p.content.map { $0.revision(p) } } }
    private var inspected: Bool { inspection != nil && inspection?.revision == revision }
    private var restricted: Bool { session.permission?.content?.requiresInspection == true }
    private var blocked: Bool { settling || (restricted && (!inspected || !viewerPresented || session.permission?.content?.available != true)) }

    private func closeInspection() {
        inspectionGeneration = UUID()
        if let inspection { service.closeInspection(inspection.token) }
        inspection = nil
        loadingInspection = false
        viewerPresented = false
    }
    private func expandCommand() {
        guard let permission = session.permission, let revision, permission.content?.available == true else { return }
        loadingInspection = true
        inspectionFailed = false
        let generation = inspectionGeneration
        service.inspectCommand(session.id, request: permission.requestID, revision: revision) { result in
            guard generation == inspectionGeneration else {
                if let result { service.closeInspection(result.token) }
                return
            }
            loadingInspection = false
            guard let result, result.revision == revision else { inspectionFailed = true; return }
            // The viewer reports itself on screen; only then can Allow be pressed.
            inspection = result
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { reveal?() }
        }
    }
    /// Why a request that needs a full read can't be allowed yet, or what the open viewer shows.
    private func inspectionNotice(_ content: PermissionContent) -> String {
        if inspected { return text.commandOpen }
        if !content.complete { return text.commandIncomplete }
        if !content.available { return text.commandTooLarge }
        return content.truncated ? text.commandCut : text.commandControls
    }
    private func approve(_ permission: ClaudePermission, _ decision: ClaudeHookMessage.Decision, autoAllowTurn: Bool = false) {
        service.decide(session.id, request: permission.requestID, decision, revision: revision,
                       inspectionToken: inspected ? inspection?.token : nil, autoAllowTurn: autoAllowTurn)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: NotchClaudeSessionsSupport.symbol(session.phase))
                    .font(.system(size: 11, weight: .semibold))
                Text(title)
                    .font(.system(size: 12, weight: .semibold))
            }
            .foregroundStyle(session.tint)
            if let permission = session.permission, permission.asksQuestions {
                NotchClaudeQuestionForm(session: session, permission: permission, text: text, settling: settling)
                    // Answers picked for one question never carry over to the next.
                    .id(permission.requestID)
            } else {
                request
            }
        }
        .onChange(of: revision) { _, _ in closeInspection() }
        .onDisappear { closeInspection() }
        .padding(10)
        .onChange(of: session.permission?.requestID, initial: true) { old, new in
            // Every request waits a moment before it can be answered, the
            // first included, as the service does.
            guard new != nil, old != new || old == nil else { return }
            settling = true
            DispatchQueue.main.asyncAfter(deadline: .now() + NotchClaudeSessionsSupport.settleDelay) { settling = false }
        }
        .background(session.tint.opacity(0.08), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(session.tint.opacity(0.35), lineWidth: 1)
        }
    }

    private var title: String {
        if session.permission?.asksQuestions == true { return text.question(for: session.provider) }
        // Codex waits on a request the island can't answer; the card's body says to answer it there.
        if session.waitingFor == CodexSessionState.answerInTerminal { return text.phase(session.phase) }
        if session.permission?.tool == ClaudeToolSummary.planTool { return text.plan }
        return session.permission?.tool ?? session.currentTool?.name ?? session.waitingFor ?? text.phase(session.phase)
    }

    /// A tool's request, read whole and allowed or denied.
    @ViewBuilder private var request: some View {
        if restricted, let content = session.permission?.content {
            HStack(alignment: .center, spacing: 8) {
                Image(systemName: inspected ? "eye" : "exclamationmark.shield.fill")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(inspected ? Color.white.opacity(0.55) : .orange)
                Text(inspectionNotice(content))
                    .font(.system(size: 10.5))
                    .foregroundStyle(.white.opacity(0.75))
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                if content.available {
                    NotchClaudeDecisionButton(
                        title: inspected ? text.hideCommand : loadingInspection ? text.openingCommand : text.showCommand,
                        symbol: inspected ? "chevron.up" : "text.magnifyingglass", role: .secondary) {
                        if inspected { closeInspection() } else { expandCommand() }
                    }
                    .disabled(loadingInspection)
                }
            }
            if inspectionFailed {
                Text(text.commandOpenFailed).font(.system(size: 10.5)).foregroundStyle(.orange)
            }
        }
        if inspected, let inspection {
            // Every character, wrapped and scrolled, so Allow lands on what was read.
            NotchFullCommandView(text: inspection.text, shown: { viewerPresented = true })
                // As tall as a diff: on the island the buttons stay in sight below it.
                .frame(height: diffHeight)
                .padding(8)
                .background(.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .accessibilityLabel(text.commandOpen)
        } else         if let diffs = session.permission?.diffs, !diffs.isEmpty {
            // A file change shows what changes, not just where.
            NotchClaudeFileDiffView(diffs: diffs, text: text, maxHeight: diffHeight)
                // Gaps opened for one request start closed on the next.
                .id(session.permission?.requestID)
        } else if let detail = session.permission?.detail, !detail.isEmpty {
            // A plan is prose, read in the interface's font.
            let plan = session.permission?.tool == ClaudeToolSummary.planTool
            // The whole request, every line of it, so nothing is allowed unread.
            ScrollView {
                Text(verbatim: detail)
                    .font(.system(size: 11, design: plan ? .default : .monospaced))
                    .foregroundStyle(.white.opacity(0.9))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxHeight: restricted ? diffHeight : 120)
            .padding(8)
            .background(.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        } else if session.permission == nil, !session.waitingDiffs.isEmpty {
            // A file change only the agent can answer still shows what it changes.
            NotchClaudeFileDiffView(diffs: session.waitingDiffs, text: text, maxHeight: diffHeight)
        }
        decisionControls
    }
    @ViewBuilder private var decisionControls: some View {
        HStack(spacing: 6) {
            if let permission = session.permission {
                let plan = permission.tool == ClaudeToolSummary.planTool
                NotchClaudeDecisionButton(title: plan ? text.keepPlanning : text.deny, symbol: "xmark", role: .deny) {
                    service.decide(session.id, request: permission.requestID, .deny)
                }
                .disabled(settling)
                Spacer(minLength: 0)
                if session.provider == .codex {
                    if restricted {
                        NotchClaudeDecisionButton(title: text.alwaysAllow, symbol: "checkmark.seal", role: .secondary) {
                            approve(permission, .allow, autoAllowTurn: true)
                        }.disabled(blocked)
                    } else {
                        NotchClaudeAutoAllowButton(session: session, text: text).disabled(blocked)
                    }
                } else if permission.offersAlways {
                    NotchClaudeDecisionButton(title: text.alwaysAllow, symbol: "checkmark.seal", role: .secondary,
                                              help: text.alwaysAllow + " (⌘T)") {
                        approve(permission, .allowAlways)
                    }
                    .disabled(blocked)
                }
                if plan, let first = permission.planModes.first {
                    // As Claude Code's plan dialog: the mode that lets Claude
                    // work on its own first, then approving each edit.
                    ForEach(Array(permission.planModes.dropFirst().reversed()), id: \.self) { mode in
                        NotchClaudeDecisionButton(title: text.planMode(mode), symbol: "hand.raised", role: .secondary,
                                                  help: text.planModeHelp(mode)) {
                            approve(permission, .approvePlan(mode))
                        }
                        .disabled(blocked)
                    }
                    NotchClaudeDecisionButton(title: text.planMode(first), symbol: "checkmark", role: .allow,
                                              help: text.planModeHelp(first) + " (⌘Y)") {
                        approve(permission, .approvePlan(first))
                    }
                    .disabled(blocked)
                } else {
                    NotchClaudeDecisionButton(title: plan ? text.approvePlan : text.allow, symbol: "checkmark", role: .allow) {
                        approve(permission, .allow)
                    }
                    .disabled(blocked)
                }
            } else {
                Text(text.answerIn(session.provider)).font(.system(size: 10.5)).foregroundStyle(.secondary)
                Spacer(minLength: 0)
                NotchClaudeDecisionButton(title: text.open, symbol: "arrow.up.forward.app", role: .secondary, action: open)
            }
        }
        if let permission = session.permission {
            Text(restricted ? (permission.content?.available == true ? text.shortcutsHintInspect : text.shortcutsHintDenyOnly)
                 : permission.planModes.first.map { String(format: text.shortcutsHintPlanFormat, text.planMode($0)) }
                 ?? (session.provider == .codex || permission.offersAlways ? text.shortcutsHintAlways : text.shortcutsHint))
                .font(.system(size: 9.5)).foregroundStyle(.tertiary)
        }
    }

}

/// The whole command in the card: read-only, selectable, wrapped, scrolled.
/// A text view, not a SwiftUI Text, so a command of up to a megabyte stays smooth.
private struct NotchFullCommandView: NSViewRepresentable {
    let text: String
    let shown: () -> Void

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.scrollerKnobStyle = .light
        if let view = scroll.documentView as? NSTextView {
            view.isEditable = false
            view.isSelectable = true
            view.drawsBackground = false
            view.isRichText = false
            view.textContainerInset = .zero
            view.textContainer?.lineFragmentPadding = 0
            view.textContainer?.widthTracksTextView = true
            fill(view)
        }
        DispatchQueue.main.async(execute: shown)
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let view = scroll.documentView as? NSTextView, view.string != text else { return }
        fill(view)
    }

    private func fill(_ view: NSTextView) {
        view.textStorage?.setAttributedString(NSAttributedString(string: text, attributes: [
            .font: NSFont.monospacedSystemFont(ofSize: 11, weight: .regular),
            .foregroundColor: NSColor.white.withAlphaComponent(0.9),
        ]))
    }
}

/// Claude's questions with the answers it wrote for them. One question with
/// a single choice is answered by clicking it, as in the terminal; otherwise
/// the person picks for each and sends them together. Their own words can
/// stand in for, or join, the choices.
struct NotchClaudeQuestionForm: View {
    let session: ClaudeSession
    let permission: ClaudePermission
    let text: NotchClaudeSessionStrings
    /// The request just appeared, so clicks wait a moment.
    let settling: Bool
    @ObservedObject private var service = ClaudeSessionService.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var picked: [Int: Set<String>] = [:]
    @State private var typed: [Int: String] = [:]
    @State private var sent = false

    private var questions: [ClaudeQuestion] { permission.questions }
    /// A click on an option answers at once, unless options carry previews
    /// that the person picks to see.
    private var answersOnClick: Bool {
        questions.count == 1 && !questions[0].multiSelect && questions[0].options.allSatisfy { $0.preview == nil }
    }
    private var answers: [String: [String]]? { NotchClaudeSessionsSupport.answers(questions, picked: picked, typed: typed) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(questions.enumerated()), id: \.offset) { index, question in
                questionView(question, index: index)
            }
            HStack(spacing: 6) {
                NotchClaudeDecisionButton(title: text.skip, symbol: "forward", role: .deny) {
                    service.decide(session.id, request: permission.requestID, .deny)
                }
                .disabled(settling)
                Spacer(minLength: 0)
                NotchClaudeDecisionButton(title: text.sendAnswer, symbol: "arrow.up", role: .allow, action: send)
                    .disabled(answers == nil)
            }
            .disabled(settling || sent)
            Text(text.questionHint(for: session.provider)).font(.system(size: 9.5)).foregroundStyle(.tertiary)
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.16), value: picked)
    }

    private func questionView(_ question: ClaudeQuestion, index: Int) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let header = question.header {
                Text(header.uppercased())
                    .font(.system(size: 9, weight: .semibold))
                    .tracking(0.6)
                    .foregroundStyle(.white.opacity(0.55))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(.white.opacity(0.08), in: Capsule())
            }
            Text(question.question)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.white.opacity(0.95))
                .fixedSize(horizontal: false, vertical: true)
                .textSelection(.enabled)
            if question.multiSelect {
                Text(text.pickAny).font(.system(size: 10)).foregroundStyle(.secondary)
            }
            VStack(spacing: 4) {
                ForEach(Array(question.options.enumerated()), id: \.offset) { _, option in
                    optionRow(option, question: question, index: index)
                }
            }
            if let preview = question.options.first(where: { picked[index]?.contains($0.label) == true })?.preview {
                ScrollView {
                    Text(preview)
                        .font(.system(size: 10.5, design: .monospaced))
                        .foregroundStyle(.white.opacity(0.85))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .frame(maxHeight: 140)
                .padding(8)
                .background(.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .transition(.opacity)
            }
            ownAnswer(question, index: index)
        }
    }

    private func optionRow(_ option: ClaudeQuestion.Option, question: ClaudeQuestion, index: Int) -> some View {
        let selected = picked[index]?.contains(option.label) == true
        let tint = session.tint
        return Button {
            pick(option.label, in: question, index: index)
        } label: {
            HStack(alignment: .top, spacing: 8) {
                marker(selected: selected, multiple: question.multiSelect)
                    .padding(.top, 1)
                VStack(alignment: .leading, spacing: 2) {
                    Text(option.label)
                        .font(.system(size: 11.5, weight: .semibold))
                        .foregroundStyle(.white.opacity(0.95))
                    if let description = option.description {
                        Text(description)
                            .font(.system(size: 10.5))
                            .foregroundStyle(.white.opacity(0.6))
                            .lineLimit(3)
                    }
                }
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
            .background(selected ? tint.opacity(0.18) : .white.opacity(0.06),
                        in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .strokeBorder(tint.opacity(selected ? 0.6 : 0), lineWidth: 1)
            }
            .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 9, lifts: false))
        .disabled(settling || sent)
        .accessibilityLabel([option.label, option.description].compactMap { $0 }.joined(separator: ", "))
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    /// A box for one of many, a dot for one of a few.
    private func marker(selected: Bool, multiple: Bool) -> some View {
        Image(systemName: multiple ? (selected ? "checkmark.square.fill" : "square")
              : (selected ? "largecircle.fill.circle" : "circle"))
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(selected ? session.tint : .white.opacity(0.45))
            .frame(width: 16, height: 16)
    }

    private func ownAnswer(_ question: ClaudeQuestion, index: Int) -> some View {
        let binding = Binding<String>(get: { typed[index] ?? "" }, set: { value in
            typed[index] = value
            // A single choice is either picked or written, not both.
            if !question.multiSelect, !value.trimmingCharacters(in: .whitespaces).isEmpty { picked[index] = nil }
        })
        return TextField(question.options.isEmpty ? text.replyPlaceholder(for: session.provider) : text.ownAnswer, text: binding)
            .textFieldStyle(.plain)
            .font(.system(size: 11.5))
            .padding(.horizontal, 9)
            .frame(height: 28)
            .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            .onSubmit(send)
            .disabled(settling || sent)
            .accessibilityLabel(text.ownAnswer)
    }

    private func pick(_ label: String, in question: ClaudeQuestion, index: Int) {
        var chosen = picked[index] ?? []
        if question.multiSelect {
            if chosen.contains(label) { chosen.remove(label) } else { chosen.insert(label) }
        } else {
            chosen = [label]
            typed[index] = nil
        }
        picked[index] = chosen.isEmpty ? nil : chosen
        if answersOnClick { send() }
    }

    private func send() {
        guard !settling, !sent, let answers else { return }
        sent = true
        service.decide(session.id, request: permission.requestID, .answer(answers))
        // The question stays only if the answer did not land, as when it was
        // just answered in the terminal; then it can be tried again.
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { sent = false }
    }
}

struct NotchClaudeDecisionButton: View {
    enum Role { case allow, deny, secondary }

    let title: String
    let symbol: String
    let role: Role
    var compact = false
    var help: String? = nil
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: symbol).font(.system(size: compact ? 9 : 10, weight: .bold))
                if !compact { Text(title).font(.system(size: 11.5, weight: .semibold)).lineLimit(1) }
            }
            .foregroundStyle(role == .allow ? Color.black.opacity(0.85) : .white.opacity(0.9))
            .padding(.horizontal, compact ? 0 : 11)
            .frame(width: compact ? 28 : nil, height: 28)
            .background(background, in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 14))
        .help(help ?? title)
        .accessibilityLabel(title)
    }

    static let allowColor = Color(red: 0.36, green: 0.84, blue: 0.52)

    private var background: Color {
        switch role {
        case .allow: return Self.allowColor
        case .deny: return Color.red.opacity(0.32)
        case .secondary: return .white.opacity(0.12)
        }
    }
}

/// Always on a Codex card: every request of the task under way is allowed
/// as it comes, this one too, until the task ends. Pressed again, it stops.
struct NotchClaudeAutoAllowButton: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared

    var body: some View {
        let on = service.autoAllowed.contains(session.id)
        NotchClaudeDecisionButton(title: on ? text.alwaysOn : text.alwaysAllow, symbol: "checkmark.seal",
                                  role: on ? .allow : .secondary, help: on ? text.alwaysOffHelp : text.alwaysOnHelp) {
            service.setAutoAllow(session.id)
        }
        .disabled(!NotchClaudeSessionsSupport.canAutoAllow(session))
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// While Always allows a Codex task's requests, the page says so, with the
/// way to stop it. It is turned on only from a request's card.
struct NotchClaudeAutoAllowBar: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared

    var body: some View {
        // A card that asks still shows its own controls; the bar stays above it.
        if session.provider == .codex, service.autoAllowed.contains(session.id),
           !session.phase.isWaiting || session.permission == nil || session.permission?.asksQuestions == true {
            // A slim strip across the page, as tall as the Windows one.
            HStack(spacing: 8) {
                HStack(spacing: 5) {
                    Image(systemName: "checkmark.seal.fill").font(.system(size: 11, weight: .semibold))
                    Text(text.alwaysOn).font(.system(size: 10, weight: .semibold)).lineLimit(1)
                }
                .accessibilityElement(children: .combine)
                Spacer(minLength: 0)
                Button { service.setAutoAllow(session.id, false) } label: {
                    Text(text.cancelAlways)
                        .font(.system(size: 11, weight: .semibold))
                        .lineLimit(1)
                        .fixedSize()
                        .padding(.horizontal, 10)
                        .frame(height: 22)
                        .background(.black.opacity(0.12), in: Capsule())
                        .contentShape(Capsule())
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 11))
                .help(text.cancelAlwaysHelp)
            }
            .foregroundStyle(Color(red: 0.04, green: 0.14, blue: 0.07))
            .padding(.leading, 9)
            .padding(.trailing, 6)
            .frame(maxWidth: .infinity, minHeight: 28)
            .background(NotchClaudeDecisionButton.allowColor, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .transition(.opacity.combined(with: .scale(scale: 0.97, anchor: .top)))
        }
    }
}

/// A reply typed here goes to the session's terminal as if typed there.
/// What is written stays per session while the island closes.
struct NotchClaudeSessionReplyField: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @FocusState private var focused: Bool
    @State private var sending = false
    @State private var problem: String?
    @State private var dropping = false
    @ObservedObject private var voice = VoiceReplyRecorder.shared
    @ObservedObject private var l10n = L10n.shared
    @AppStorage(DefaultsKey.notchSessionsVoiceReplies) private var voiceReplies = true

    private var draft: Binding<String> {
        Binding(get: { service.drafts[session.id] ?? "" }, set: { service.drafts[session.id] = $0 })
    }

    private var recording: Bool { voice.isRecording(session) }

    private var placeholder: String {
        if recording { return NotchVoiceReplyStrings.forLanguage(l10n.language).listening }
        return unavailable ?? text.replyPlaceholder(for: session.provider)
    }

    private var unavailable: String? {
        if session.phase.isWaiting { return text.replyWhileWaiting }
        if session.waitingFor != nil { return text.replyWhileDialog }
        if session.background?.ended == true { return text.replyEnded }
        // Codex and OpenCode take replies through their own channel, wherever their terminal is.
        if !NotchClaudeSessionsSupport.repliesReach(session) { return text.replyUnavailable(for: session) }
        return nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            NotchClaudeAttachmentStrip(sessionID: session.id, text: text, side: 36)
            HStack(spacing: 6) {
                TextField(placeholder, text: draft)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12))
                    .focused($focused)
                    .onSubmit(send)
                    // The words fill in by themselves while it listens.
                    .disabled(unavailable != nil || sending || recording)
                    .accessibilityLabel(text.replyPlaceholder(for: session.provider))
                if VoiceReplySupport.showsMicrophone(for: session, enabled: voiceReplies) {
                    NotchVoiceReplyButton(session: session, side: 24)
                        .disabled(unavailable != nil || sending)
                }
                Button(action: send) {
                    Group {
                        if sending {
                            ProgressView().controlSize(.mini)
                        } else {
                            Image(systemName: "arrow.up").font(.system(size: 11, weight: .bold))
                        }
                    }
                    .foregroundStyle(canSend ? Color.black.opacity(0.85) : .white.opacity(0.35))
                    .frame(width: 24, height: 24)
                    .background(canSend ? session.provider.tint : .white.opacity(0.08), in: Circle())
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 12))
                // Return sends from the field, and ⌘↩ as on the enlarged page.
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(!canSend)
                .help(text.send + " · " + text.sendShortcut)
                .accessibilityLabel(text.send)
            }
            .padding(.leading, 10)
            .padding(.trailing, 4)
            .frame(height: 32)
            .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(.white.opacity(dropping ? 0.6 : focused ? 0.3 : 0), lineWidth: 1)
            }
            .modifier(NotchClaudeImageInput(sessionID: session.id, dropping: $dropping))
            if let problem {
                Label(problem, systemImage: "exclamationmark.triangle.fill")
                    .font(.system(size: 10))
                    .foregroundStyle(.orange)
                    .transition(.opacity)
            }
            NotchVoiceReplyProblemLabel(session: session, text: text, fontSize: 10)
        }
        .onChange(of: session.id) { _, _ in problem = nil }
    }

    private var canSend: Bool {
        unavailable == nil && !sending && !recording
            && (NotchClaudeSessionsSupport.reply(draft.wrappedValue) != nil || service.attachments[session.id] != nil)
    }

    private func send() {
        guard canSend else { return }
        sending = true
        problem = nil
        service.send(draft.wrappedValue, to: session) { failure in
            sending = false
            problem = failure.map { NotchClaudeSessionFormat.problem($0, text: text) }
        }
    }
}
