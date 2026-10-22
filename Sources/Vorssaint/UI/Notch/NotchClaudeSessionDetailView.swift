// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

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

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }
    /// Working subagents come first, then the latest done.
    static let shownSubagents = 4

    var body: some View {
        VStack(spacing: 8) {
            if renaming { nameField }
            if notch.sessionsEnlarged {
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
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    header(now: context.date)
                    if session.phase.isWaiting { NotchClaudeSessionPermissionCard(session: session, text: text, open: open) }
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

    // MARK: Header

    private func header(now: Date) -> some View {
        HStack(spacing: 10) {
            NotchClaudeSessionBadge(phase: session.phase, size: 30)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(text.phase(session.phase))
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(session.phase.tint)
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
                    NotchClaudeProjectTag(project: session.project)
                    Text(details)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
    }

    private var details: String {
        [session.branch.map { "⎇ \($0)" }, session.model.map(AgentPricing.displayName),
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
        .help(text.open)
    }

    // MARK: Work

    private func tasksRow(_ tasks: ClaudeTaskProgress) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(text.tasks(done: tasks.done, total: tasks.total))
                    .font(.system(size: 10.5, weight: .medium))
                    .foregroundStyle(.white.opacity(0.8))
                if let current = tasks.current {
                    Text(current).font(.system(size: 10.5)).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(.white.opacity(0.1))
                    Capsule().fill(AgentProvider.claude.tint)
                        .frame(width: max(4, proxy.size.width * tasks.fraction))
                }
            }
            .frame(height: 4)
            .accessibilityHidden(true)
        }
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
                .foregroundStyle(tool.isRunning ? AgentProvider.claude.tint : (tool.failed ? .red : .white.opacity(0.45)))
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
                    Image(systemName: agent.isRunning ? "arrow.triangle.branch" : "checkmark")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(agent.isRunning ? AgentProvider.claude.tint : .white.opacity(0.45))
                        .frame(width: 12)
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
            Text(text.previewsOff).font(.system(size: 10)).foregroundStyle(.tertiary)
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
    @ObservedObject private var service = ClaudeSessionService.shared
    /// A request that replaced the one on screen holds the buttons back a
    /// moment, so a click meant for the old one cannot answer it.
    @State private var settling = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: NotchClaudeSessionsSupport.symbol(session.phase))
                    .font(.system(size: 11, weight: .semibold))
                Text(session.permission?.tool ?? session.currentTool?.name ?? session.waitingFor ?? text.phase(session.phase))
                    .font(.system(size: 12, weight: .semibold))
            }
            .foregroundStyle(session.phase.tint)
            if let detail = session.permission?.detail, !detail.isEmpty {
                // The whole request, every line of it, so nothing is allowed unread.
                ScrollView {
                    Text(detail)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.white.opacity(0.9))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .frame(maxHeight: 120)
                .padding(8)
                .background(.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            }
            HStack(spacing: 6) {
                if let permission = session.permission {
                    NotchClaudeDecisionButton(title: text.deny, symbol: "xmark", role: .deny) {
                        service.decide(session.id, request: permission.requestID, .deny)
                    }
                    .disabled(settling)
                    Spacer(minLength: 0)
                    if permission.offersAlways {
                        NotchClaudeDecisionButton(title: text.alwaysAllow, symbol: "checkmark.seal", role: .secondary) {
                            service.decide(session.id, request: permission.requestID, .allowAlways)
                        }
                        .disabled(settling)
                    }
                    NotchClaudeDecisionButton(title: text.allow, symbol: "checkmark", role: .allow) {
                        service.decide(session.id, request: permission.requestID, .allow)
                    }
                    .disabled(settling)
                } else {
                    Text(text.answerInTerminal).font(.system(size: 10.5)).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                    NotchClaudeDecisionButton(title: text.open, symbol: "arrow.up.forward.app", role: .secondary, action: open)
                }
            }
            if session.permission != nil {
                Text(text.shortcutsHint).font(.system(size: 9.5)).foregroundStyle(.tertiary)
            }
        }
        .padding(10)
        .onChange(of: session.permission?.requestID, initial: true) { old, new in
            // Every request waits a moment before it can be answered, the
            // first included, as the service does.
            guard new != nil, old != new || old == nil else { return }
            settling = true
            DispatchQueue.main.asyncAfter(deadline: .now() + NotchClaudeSessionsSupport.settleDelay) { settling = false }
        }
        .background(session.phase.tint.opacity(0.08), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(session.phase.tint.opacity(0.35), lineWidth: 1)
        }
    }
}

struct NotchClaudeDecisionButton: View {
    enum Role { case allow, deny, secondary }

    let title: String
    let symbol: String
    let role: Role
    var compact = false
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
        .help(title)
        .accessibilityLabel(title)
    }

    private var background: Color {
        switch role {
        case .allow: return Color(red: 0.36, green: 0.84, blue: 0.52)
        case .deny: return Color.red.opacity(0.32)
        case .secondary: return .white.opacity(0.12)
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

    private var draft: Binding<String> {
        Binding(get: { service.drafts[session.id] ?? "" }, set: { service.drafts[session.id] = $0 })
    }

    private var unavailable: String? {
        if session.phase.isWaiting { return text.replyWhileWaiting }
        if session.waitingFor != nil { return text.replyWhileDialog }
        if !session.host.acceptsReplies { return text.replyUnavailable }
        return nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            NotchClaudeAttachmentStrip(sessionID: session.id, text: text, side: 36)
            HStack(spacing: 6) {
                TextField(unavailable ?? text.replyPlaceholder, text: draft)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12))
                    .focused($focused)
                    .onSubmit(send)
                    .disabled(unavailable != nil || sending)
                    .accessibilityLabel(text.replyPlaceholder)
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
                    .background(canSend ? AgentProvider.claude.tint : .white.opacity(0.08), in: Circle())
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 12))
                .disabled(!canSend)
                .help(text.send)
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
        }
        .onChange(of: session.id) { _, _ in problem = nil }
    }

    private var canSend: Bool {
        unavailable == nil && !sending
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
