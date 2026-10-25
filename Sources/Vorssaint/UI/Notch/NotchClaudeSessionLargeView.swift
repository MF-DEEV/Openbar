// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

/// A session's page with the island grown around it: the conversation so
/// far beside what the session is doing, at a text size the person picks,
/// and a reply written over several lines.
struct NotchClaudeSessionLargeView: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    let open: () -> Void
    @AppStorage(DefaultsKey.notchSessionsLargeTextSize) private var textSize = NotchClaudeSessionsSupport.largeTextSizes.lowerBound + 3
    @AppStorage(DefaultsKey.notchSessionsContentPreview) private var previews = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage(DefaultsKey.notchSessionsSidebarHidden) private var storedSidebarHidden = false
    /// The page's own copy, so folding animates; a change read back from the
    /// preferences arrives outside the click's animation and would jump.
    @State private var sidebarHidden = UserDefaults.standard.bool(forKey: DefaultsKey.notchSessionsSidebarHidden)
    /// The room the conversation leaves the details, changed at once.
    @State private var layoutHidden = UserDefaults.standard.bool(forKey: DefaultsKey.notchSessionsSidebarHidden)
    @State private var textSettling = false
    @State private var allTools = false
    @State private var allAgents = false
    @State private var handleHovered = false
    @State private var pageHeight: CGFloat = 0
    /// The header's and the reply's heights, which never give way to the card.
    @State private var headerHeight: CGFloat = 0
    @State private var composerHeight: CGFloat = 0

    private static let thinkingRow = -1
    /// The most of the page a request or Claude's questions take before they scroll.
    static let cardShare: CGFloat = 0.6
    /// What the card leaves the conversation, and keeps for itself, on a short page.
    static let conversationFloor: CGFloat = 80
    static let cardFloor: CGFloat = 120
    static let sidebarWidth: CGFloat = 330
    /// The handle's column, which stays when the details fold away.
    static let handleWidth: CGFloat = 38
    /// Rows a folded list shows: the running ones and the latest.
    static let foldedRows = 4
    private var size: CGFloat { CGFloat(NotchClaudeSessionsSupport.largeTextSize(textSize)) }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            VStack(alignment: .leading, spacing: 12) {
                header(now: context.date)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { headerHeight = $0 }
                if session.phase.isWaiting {
                    // Several questions run taller than the page; past most of
                    // it they scroll, leaving the conversation and the reply.
                    // The next request, or another session's, opens at its top.
                    NotchClaudeSessionPermissionCard(session: session, text: text, open: open)
                        .modifier(NotchClaudeFittedScroll(maxHeight: cardLimit,
                                                          start: [session.id, session.permission?.requestID ?? ""]))
                }
                // Text that changes width on every frame breaks its lines on
                // every frame. So the conversation takes its new width at
                // once, breaking its lines a single time under a short fade,
                // while the details slide over it as a layer of their own.
                HStack(alignment: .top, spacing: 0) {
                    conversation
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                        .opacity(textSettling ? 0.25 : 1)
                    Color.clear
                        .frame(width: Self.handleWidth + (layoutHidden ? 0 : Self.sidebarWidth))
                }
                .overlay(alignment: .trailing) {
                    HStack(spacing: 0) {
                        sidebarHandle
                        ScrollView { sidebar(now: context.date).padding(.bottom, 8) }
                            .scrollIndicators(.automatic)
                            .frame(width: Self.sidebarWidth)
                            .opacity(sidebarHidden ? 0 : 1)
                            .allowsHitTesting(!sidebarHidden)
                            .accessibilityHidden(sidebarHidden)
                    }
                    .frame(maxHeight: .infinity)
                    .background(Color.black)
                    .offset(x: sidebarHidden ? Self.sidebarWidth : 0)
                }
                .clipped()
                .frame(maxHeight: .infinity)
                NotchClaudeSessionComposer(session: session, text: text, size: size)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { composerHeight = $0 }
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { pageHeight = $0 }
        }
        .background {
            // ⌘+ and ⌘- change the text size, as in a browser.
            Group {
                Button("") { resize(by: 1) }.keyboardShortcut("+", modifiers: .command)
                Button("") { resize(by: 1) }.keyboardShortcut("=", modifiers: .command)
                Button("") { resize(by: -1) }.keyboardShortcut("-", modifiers: .command)
            }
            .opacity(0)
            .accessibilityHidden(true)
        }
    }

    /// Most of the page, but never so much that large text or added images
    /// push the reply off a short screen.
    private var cardLimit: CGFloat {
        guard pageHeight > 0 else { return 360 }
        // The page's three gaps around the card and the conversation.
        let room = pageHeight - headerHeight - composerHeight - 3 * 12 - Self.conversationFloor
        return max(Self.cardFloor, min(pageHeight * Self.cardShare, room))
    }

    private func toggleSidebar() {
        let hiding = !sidebarHidden
        storedSidebarHidden = hiding
        guard !reduceMotion else {
            layoutHidden = hiding
            sidebarHidden = hiding
            return
        }
        // First the conversation's new width, with no animation and its
        // text dimmed; then, in the next update, the details slide and the
        // text comes back.
        var still = Transaction(animation: nil)
        still.disablesAnimations = true
        withTransaction(still) {
            layoutHidden = hiding
            textSettling = true
        }
        DispatchQueue.main.async {
            withAnimation(.smooth(duration: 0.38)) { sidebarHidden = hiding }
            withAnimation(.easeOut(duration: 0.3)) { textSettling = false }
        }
    }

    /// The line between the conversation and the details folds the details
    /// away, or brings them back, at a click.
    private var sidebarHandle: some View {
        Button(action: toggleSidebar) {
            ZStack {
                Rectangle()
                    .fill(.white.opacity(handleHovered ? 0.22 : 0.08))
                    .frame(width: 1)
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .bold))
                    .rotationEffect(.degrees(sidebarHidden ? 180 : 0))
                    .foregroundStyle(.white.opacity(handleHovered ? 0.9 : 0.5))
                    .frame(width: 18, height: 34)
                    .background(Color(white: handleHovered ? 0.2 : 0.12), in: Capsule())
            }
            .frame(width: 26)
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 6)
        .onHover { hovering in
            withAnimation(reduceMotion ? nil : .easeOut(duration: 0.12)) { handleHovered = hovering }
        }
        .help(sidebarHidden ? text.showDetails : text.hideDetails)
        .accessibilityLabel(sidebarHidden ? text.showDetails : text.hideDetails)
    }

    private func resize(by step: Double) {
        textSize = NotchClaudeSessionsSupport.largeTextSize(textSize + step)
    }

    // MARK: Header

    private func header(now: Date) -> some View {
        HStack(alignment: .center, spacing: 12) {
            NotchClawdDressButton(session: session, name: ClaudeSessionService.shared.displayName(session), text: text,
                                  size: CGSize(width: 48, height: 42), badge: 34)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(text.phase(session.phase))
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(session.tint)
                    if let clock = NotchClaudeSessionFormat.clock(session, now: now) {
                        Text(clock).font(.system(size: 14, weight: .medium)).monospacedDigit().foregroundStyle(.secondary)
                    }
                    if let tool = session.currentTool, session.phase == .runningTool {
                        Text(tool.summary.map { "\(tool.name): \($0)" } ?? tool.name)
                            .font(.system(size: 13, design: .monospaced))
                            .foregroundStyle(.white.opacity(0.75))
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                HStack(spacing: 8) {
                    NotchClaudeProjectTag(project: session.project, size: 12)
                    Text([session.branch.map { "⎇ \($0)" }, session.model.map(AgentPricing.displayName),
                          session.tokens.total > 0 ? AgentFormat.tokens(session.tokens.total) : nil,
                          session.cost > 0 ? AgentFormat.cost(session.cost) : nil, session.cwd]
                            .compactMap { $0 }.joined(separator: " · "))
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                }
            }
            Spacer(minLength: 12)
            HStack(spacing: 2) {
                sizeButton("textformat.size.smaller", help: text.smallerText, step: -1)
                sizeButton("textformat.size.larger", help: text.largerText, step: 1)
                Button(action: toggleSidebar) {
                    Image(systemName: "sidebar.right")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(.white.opacity(sidebarHidden ? 0.55 : 0.9))
                        .frame(width: 30, height: 30)
                        .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 9))
                .help(sidebarHidden ? text.showDetails : text.hideDetails)
                .accessibilityLabel(sidebarHidden ? text.showDetails : text.hideDetails)
            }
        }
    }

    private func sizeButton(_ symbol: String, help: String, step: Double) -> some View {
        Button { resize(by: step) } label: {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.white.opacity(0.85))
                .frame(width: 30, height: 30)
                .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 9))
        .disabled(NotchClaudeSessionsSupport.largeTextSize(textSize + step) == textSize)
        .help(help)
        .accessibilityLabel(help)
    }

    // MARK: Conversation

    @ViewBuilder private var conversation: some View {
        if !previews {
            VStack(alignment: .leading, spacing: 8) {
                Text(text.previewsOff(for: session.provider)).font(.system(size: size - 1)).foregroundStyle(.tertiary)
                Button(text.contentPreview) {
                    previews = true
                    ClaudeSessionService.shared.syncWithPreferences()
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 8))
                .help(text.contentPreviewHint)
            }
        } else if session.chat.isEmpty && !session.phase.isWorking {
            Text(text.noMessages).font(.system(size: size - 1)).foregroundStyle(.tertiary)
        } else {
            let marks = NotchClaudeSessionsSupport.answers(in: session.chat)
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: size) {
                        ForEach(session.chat) { message in
                            bubble(message, answer: marks.answers.contains(message.id),
                                   onTheWay: marks.onTheWay.contains(message.id))
                                .id(message.id)
                        }
                        if session.phase.isWorking {
                            NotchClaudeThinkingRow(session: session, size: size)
                                .id(Self.thinkingRow)
                                .transition(.opacity)
                        }
                    }
                    .padding(.trailing, 6)
                    .padding(.vertical, 4)
                }
                .scrollIndicators(.automatic)
                .defaultScrollAnchor(.bottom)
                .modifier(NotchClaudeBottomHeld())
                .accessibilityLabel(text.conversation)
                // A new message brings the conversation to its end.
                .onChange(of: session.chat.last?.id) { _, id in
                    guard let id else { return }
                    withAnimation(reduceMotion ? nil : .easeOut(duration: 0.2)) {
                        proxy.scrollTo(session.phase.isWorking ? Self.thinkingRow : id, anchor: .bottom)
                    }
                }
                .onChange(of: session.phase.isWorking) { _, working in
                    guard working else { return }
                    withAnimation(reduceMotion ? nil : .easeOut(duration: 0.2)) { proxy.scrollTo(Self.thinkingRow, anchor: .bottom) }
                }
            }
        }
    }

    @ViewBuilder private func bubble(_ message: ClaudeChatMessage, answer: Bool, onTheWay: Bool) -> some View {
        switch message.role {
        case .user:
            VStack(alignment: .trailing, spacing: 4) {
                Text(message.text)
                    .font(.system(size: size))
                    .foregroundStyle(.white.opacity(0.95))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .background(.white.opacity(0.1), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                Text(message.at, style: .time).font(.system(size: 10)).foregroundStyle(.tertiary)
            }
            .padding(.leading, 60)
            .frame(maxWidth: .infinity, alignment: .trailing)
        case .assistant where answer:
            // The reply a turn ended on stands apart from the ones written
            // on the way, under a rule of its own.
            VStack(alignment: .leading, spacing: size * 0.6) {
                answerRule(message)
                NotchClaudeRichText(text: NotchClaudeSessionFormat.markdown(message.text), size: size,
                                    color: .white.withAlphaComponent(0.96), lineSpacing: size * 0.25)
            }
            .padding(.top, size * 0.5)
            .frame(maxWidth: .infinity, alignment: .leading)
        case .assistant:
            NotchClaudeRichText(text: NotchClaudeSessionFormat.markdown(message.text), size: size,
                                color: .white.withAlphaComponent(onTheWay ? 0.6 : 0.88), lineSpacing: size * 0.25)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func answerRule(_ message: ClaudeChatMessage) -> some View {
        HStack(spacing: 8) {
            Text(text.answer.uppercased())
                .font(.system(size: max(10, size - 4), weight: .semibold))
                .tracking(0.8)
                .foregroundStyle(session.provider.tint)
            Text(message.at, style: .time)
                .font(.system(size: max(10, size - 4)))
                .foregroundStyle(.tertiary)
            Rectangle()
                .fill(session.provider.tint.opacity(0.35))
                .frame(height: 1)
        }
        .accessibilityElement(children: .combine)
    }

    // MARK: Sidebar

    private func sidebar(now: Date) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            if let title = session.title {
                Text(title)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.95))
                    .textSelection(.enabled)
            }
            if let tasks = session.tasks { tasksRow(tasks) }
            if !session.timeline.isEmpty {
                let tools = Array(session.timeline.reversed())
                fold(text.recentTools, count: tools.count, expanded: $allTools) {
                    ForEach(tools.prefix(allTools ? tools.count : Self.foldedRows)) { toolRow($0) }
                }
            }
            if !session.subagents.isEmpty {
                fold(text.subagents, count: session.subagents.count, expanded: $allAgents) {
                    ForEach(session.subagents.prefix(allAgents ? session.subagents.count : Self.foldedRows)) { agentRow($0, now: now) }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A list that shows its first rows and opens to all of them.
    private func fold<Rows: View>(_ title: String, count: Int, expanded: Binding<Bool>,
                                  @ViewBuilder rows: () -> Rows) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title.uppercased())
                .font(.system(size: 11, weight: .semibold))
                .tracking(0.8)
                .foregroundStyle(.white.opacity(0.45))
            rows()
            if count > Self.foldedRows {
                Button {
                    withAnimation(reduceMotion ? nil : .easeOut(duration: 0.18)) { expanded.wrappedValue.toggle() }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: expanded.wrappedValue ? "chevron.up" : "chevron.down")
                            .font(.system(size: 9, weight: .bold))
                        Text(expanded.wrappedValue ? text.showLess : text.showAll(count))
                    }
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.white.opacity(0.6))
                    .padding(.horizontal, 8)
                    .frame(height: 24)
                    .contentShape(Capsule())
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 12))
            }
        }
    }

    private func tasksRow(_ tasks: ClaudeTaskProgress) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(text.tasks(done: tasks.done, total: tasks.total)).font(.system(size: 12, weight: .medium))
            if let current = tasks.current {
                Text(current).font(.system(size: 12)).foregroundStyle(.secondary).lineLimit(2)
            }
            ProgressView(value: tasks.fraction).tint(session.provider.tint)
        }
    }

    private func toolRow(_ tool: ClaudeToolUse) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Image(systemName: tool.isRunning ? "circle.dotted" : (tool.failed ? "xmark.circle.fill" : "checkmark.circle.fill"))
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(tool.isRunning ? session.provider.tint : (tool.failed ? .red : .white.opacity(0.4)))
            Text(tool.name).font(.system(size: 12, weight: .semibold)).foregroundStyle(.white.opacity(0.9))
            Text(tool.summary ?? "")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.white.opacity(0.65))
                .lineLimit(1)
                .truncationMode(.middle)
                .help(tool.summary ?? "")
            Spacer(minLength: 4)
            if let finished = tool.finished {
                Text(AgentFormat.clock(finished.timeIntervalSince(tool.started)))
                    .font(.system(size: 10)).monospacedDigit().foregroundStyle(.secondary)
            }
        }
    }

    private func agentRow(_ agent: ClaudeSubagent, now: Date) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            NotchClaudeSubagentAvatar(agent: agent, session: session, size: CGSize(width: 22, height: 17)) {
                Image(systemName: agent.isRunning ? "arrow.triangle.branch" : "checkmark")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(agent.isRunning ? session.provider.tint : .white.opacity(0.4))
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(agent.type).font(.system(size: 12, weight: .semibold)).foregroundStyle(.white.opacity(0.9))
                if let description = agent.description {
                    Text(description).font(.system(size: 11)).foregroundStyle(.secondary).lineLimit(2)
                }
            }
            Spacer(minLength: 4)
            Text(AgentFormat.clock((agent.finished ?? now).timeIntervalSince(agent.started)))
                .font(.system(size: 10)).monospacedDigit().foregroundStyle(.secondary)
        }
    }
}

/// Claude at work, as its terminal shows it: a turning star and a word
/// with light passing over it, with how long the turn has run. A Codex
/// session shows Codex's breathing mark and color in the star's place.
struct NotchClaudeThinkingRow: View {
    let session: ClaudeSession
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var word: String { NotchClaudeSessionsSupport.thinkingWord(turn: session.turnStarted) + "…" }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 24, paused: reduceMotion)) { context in
            let time = context.date.timeIntervalSinceReferenceDate
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                glyph(at: time)
                Text(shimmer(at: time))
                    .font(.system(size: size, weight: .medium))
                if let clock = NotchClaudeSessionFormat.clock(session, now: context.date) {
                    Text(clock).font(.system(size: size - 3)).monospacedDigit().foregroundStyle(.tertiary)
                }
                if session.phase == .runningTool, let tool = session.currentTool {
                    Text(tool.name).font(.system(size: size - 3, weight: .medium)).foregroundStyle(.secondary)
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(word)
    }

    @ViewBuilder private func glyph(at time: TimeInterval) -> some View {
        switch session.provider {
        case .claude:
            Text(reduceMotion ? "✻" : NotchClaudeSessionsSupport.thinkingGlyph(step: Int(time * 8)))
                .font(.system(size: size + 1, weight: .semibold))
                .foregroundStyle(session.provider.tint)
                .frame(width: size + 4)
        case .codex:
            // As wide as the star's column; centred on the word's letters
            // rather than sat on their baseline. It breathes on its own clock
            // and stays still under Reduce Motion.
            NotchSessionAgentGlyph(provider: .codex, size: (size + 3) / 1.45)
                .frame(width: size + 4)
                .alignmentGuide(.firstTextBaseline) { $0.height / 2 + size * 0.35 }
        }
    }

    /// A band of light crossing the word from left to right, then a pause.
    private func shimmer(at time: TimeInterval) -> AttributedString {
        var result = AttributedString()
        let letters = Array(word)
        let tint = session.provider.tint
        let band = 3.0
        let span = Double(letters.count) + band * 2 + 6
        let center = (time * 14).truncatingRemainder(dividingBy: span) - band
        for (index, letter) in letters.enumerated() {
            var piece = AttributedString(String(letter))
            let glow = reduceMotion ? 0 : max(0, 1 - abs(Double(index) - center) / band)
            piece.foregroundColor = glow > 0
                ? Color(nsColor: NSColor(tint).blended(withFraction: glow * 0.7, of: .white) ?? NSColor(tint)) : tint
            result += piece
        }
        return result
    }
}

/// A reply written over several lines; ⌘↩ sends it to the session's terminal.
struct NotchClaudeSessionComposer: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    let size: CGFloat
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
        // Codex takes replies through its server, wherever its terminal is.
        if session.provider != .codex, !session.host.acceptsReplies { return text.replyUnavailable }
        return nil
    }

    private var canSend: Bool {
        unavailable == nil && !sending
            && (NotchClaudeSessionsSupport.reply(draft.wrappedValue) != nil || service.attachments[session.id] != nil)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            NotchClaudeAttachmentStrip(sessionID: session.id, text: text, side: 54)
            HStack(alignment: .bottom, spacing: 10) {
                ZStack(alignment: .topLeading) {
                    if draft.wrappedValue.isEmpty {
                        Text(unavailable ?? text.replyPlaceholder(for: session.provider))
                            .font(.system(size: size))
                            .foregroundStyle(.white.opacity(0.35))
                            .padding(.horizontal, 5)
                            .allowsHitTesting(false)
                    }
                    TextEditor(text: draft)
                        .font(.system(size: size))
                        .scrollContentBackground(.hidden)
                        .focused($focused)
                        .disabled(unavailable != nil || sending)
                        .accessibilityLabel(text.replyPlaceholder(for: session.provider))
                }
                // About three lines; the editor scrolls past that.
                .frame(height: size * 4)
                .padding(10)
                .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .strokeBorder(.white.opacity(dropping ? 0.6 : focused ? 0.3 : 0.08), lineWidth: 1)
                }
                .modifier(NotchClaudeImageInput(sessionID: session.id, takesImages: session.provider != .codex, dropping: $dropping))
                Button(action: send) {
                    Group {
                        if sending { ProgressView().controlSize(.small) } else {
                            Image(systemName: "arrow.up").font(.system(size: 15, weight: .bold))
                        }
                    }
                    .foregroundStyle(canSend ? Color.black.opacity(0.85) : .white.opacity(0.35))
                    .frame(width: 38, height: 38)
                    .background(canSend ? session.provider.tint : .white.opacity(0.08), in: Circle())
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 19))
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(!canSend)
                .help(text.send + " · " + text.sendShortcut)
                .accessibilityLabel(text.send)
            }
            HStack {
                if let problem {
                    Label(problem, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                }
                Spacer()
                Text(text.pasteImages + " · " + text.sendShortcut).foregroundStyle(.tertiary)
            }
            .font(.system(size: 11))
        }
        .onChange(of: session.id) { _, _ in problem = nil }
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

/// Content at its own height until it would pass `maxHeight`, and scrolled
/// from there. A new `start` brings it back to its top.
struct NotchClaudeFittedScroll<Start: Equatable>: ViewModifier {
    let maxHeight: CGFloat
    let start: Start
    @State private var height: CGFloat = 0
    private static var top: String { "fitted.top" }

    func body(content: Content) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                content
                    .id(Self.top)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height = $0 }
            }
            .scrollIndicators(.automatic)
            .scrollBounceBehavior(.basedOnSize)
            .frame(height: min(height, maxHeight))
            .onChange(of: start) { proxy.scrollTo(Self.top, anchor: .top) }
        }
    }
}

/// When text breaks its lines anew, as the details fold, the conversation
/// grows or shrinks; held at its end, the place being read stays put rather
/// than the view landing elsewhere in the conversation.
struct NotchClaudeBottomHeld: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 15.0, *) {
            content.defaultScrollAnchor(.bottom, for: .sizeChanges)
        } else {
            content
        }
    }
}

/// Images added to a reply, each with a way to take it off again.
struct NotchClaudeAttachmentStrip: View {
    let sessionID: String
    let text: NotchClaudeSessionStrings
    let side: CGFloat
    @ObservedObject private var service = ClaudeSessionService.shared

    var body: some View {
        if let images = service.attachments[sessionID], !images.isEmpty {
            HStack(spacing: 8) {
                ForEach(images, id: \.self) { image in
                    ZStack(alignment: .topTrailing) {
                        NotchClaudeThumbnail(url: image, side: side)
                        Button { service.detach(image, from: sessionID) } label: {
                            Image(systemName: "xmark.circle.fill")
                                .font(.system(size: 14))
                                .symbolRenderingMode(.palette)
                                .foregroundStyle(.white, .black.opacity(0.75))
                        }
                        .buttonStyle(.plain)
                        .offset(x: 5, y: -5)
                        .help(text.removeImage)
                        .accessibilityLabel(text.removeImage)
                    }
                }
            }
            .padding(.top, 5)
            .transition(.opacity)
        }
    }
}

/// A small picture of an added image, read once.
struct NotchClaudeThumbnail: View {
    let url: URL
    let side: CGFloat
    private static let cache = NSCache<NSURL, NSImage>()

    var body: some View {
        Group {
            if let image = Self.image(url) {
                Image(nsImage: image).resizable().scaledToFill()
            } else {
                Image(systemName: "photo").foregroundStyle(.secondary)
            }
        }
        .frame(width: side, height: side)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(.white.opacity(0.15), lineWidth: 1) }
        .accessibilityLabel(url.lastPathComponent)
    }

    private static func image(_ url: URL) -> NSImage? {
        if let known = cache.object(forKey: url as NSURL) { return known }
        guard let image = NSImage(contentsOf: url) else { return nil }
        cache.setObject(image, forKey: url as NSURL)
        return image
    }
}

/// Lets a reply take images: ⌘V adds the pasteboard's images, or pastes
/// text when it holds none, and images dropped on the field are added.
struct NotchClaudeImageInput: ViewModifier {
    let sessionID: String
    /// Codex takes text replies only: its field neither pastes nor takes
    /// dropped images, so none is copied aside for nothing.
    var takesImages = true
    @Binding var dropping: Bool

    @ViewBuilder func body(content: Content) -> some View {
        if takesImages {
            withImages(content)
        } else {
            content
        }
    }

    private func withImages(_ content: Content) -> some View {
        content
            .background {
                Button("") {
                    if !ClaudeSessionService.shared.attachImages(from: .general, to: sessionID) {
                        NSApp.sendAction(#selector(NSText.paste(_:)), to: nil, from: nil)
                    }
                }
                .keyboardShortcut("v", modifiers: .command)
                .opacity(0)
                .accessibilityHidden(true)
            }
            .onDrop(of: [.fileURL, .image], isTargeted: $dropping) { providers in
                ClaudeImageAttachments.images(from: providers) { ClaudeSessionService.shared.attach($0, to: sessionID) }
                return true
            }
    }
}
