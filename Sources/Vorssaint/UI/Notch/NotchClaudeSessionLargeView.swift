// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

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
    @State private var allTools = false
    @State private var allAgents = false

    /// Rows a folded list shows: the running ones and the latest.
    static let foldedRows = 4
    private var size: CGFloat { CGFloat(NotchClaudeSessionsSupport.largeTextSize(textSize)) }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            VStack(alignment: .leading, spacing: 12) {
                header(now: context.date)
                if session.phase.isWaiting { NotchClaudeSessionPermissionCard(session: session, text: text, open: open) }
                HStack(alignment: .top, spacing: 18) {
                    conversation
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    Divider().overlay(Color.white.opacity(0.08))
                    ScrollView { sidebar(now: context.date).padding(.bottom, 8) }
                        .scrollIndicators(.automatic)
                        .frame(width: 330)
                }
                .frame(maxHeight: .infinity)
                NotchClaudeSessionComposer(session: session, text: text, size: size)
            }
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

    private func resize(by step: Double) {
        textSize = NotchClaudeSessionsSupport.largeTextSize(textSize + step)
    }

    // MARK: Header

    private func header(now: Date) -> some View {
        HStack(alignment: .center, spacing: 12) {
            NotchClaudeSessionBadge(phase: session.phase, size: 34)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(text.phase(session.phase))
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(session.phase.tint)
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
            Spacer(minLength: 12)
            HStack(spacing: 2) {
                sizeButton("textformat.size.smaller", help: text.smallerText, step: -1)
                sizeButton("textformat.size.larger", help: text.largerText, step: 1)
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
            Text(text.previewsOff).font(.system(size: size - 1)).foregroundStyle(.tertiary)
        } else if session.chat.isEmpty {
            Text(text.noMessages).font(.system(size: size - 1)).foregroundStyle(.tertiary)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: size) {
                        ForEach(session.chat) { message in
                            bubble(message).id(message.id)
                        }
                    }
                    .padding(.trailing, 6)
                    .padding(.vertical, 4)
                }
                .scrollIndicators(.automatic)
                .defaultScrollAnchor(.bottom)
                .accessibilityLabel(text.conversation)
                // A new message brings the conversation to its end.
                .onChange(of: session.chat.last?.id) { _, id in
                    guard let id else { return }
                    withAnimation(reduceMotion ? nil : .easeOut(duration: 0.2)) { proxy.scrollTo(id, anchor: .bottom) }
                }
            }
        }
    }

    @ViewBuilder private func bubble(_ message: ClaudeChatMessage) -> some View {
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
        case .assistant:
            Text(NotchClaudeSessionFormat.markdown(message.text))
                .font(.system(size: size))
                .lineSpacing(size * 0.25)
                .foregroundStyle(.white.opacity(0.88))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
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
            ProgressView(value: tasks.fraction).tint(AgentProvider.claude.tint)
        }
    }

    private func toolRow(_ tool: ClaudeToolUse) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Image(systemName: tool.isRunning ? "circle.dotted" : (tool.failed ? "xmark.circle.fill" : "checkmark.circle.fill"))
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(tool.isRunning ? AgentProvider.claude.tint : (tool.failed ? .red : .white.opacity(0.4)))
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
            Image(systemName: agent.isRunning ? "arrow.triangle.branch" : "checkmark")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(agent.isRunning ? AgentProvider.claude.tint : .white.opacity(0.4))
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

/// A reply written over several lines; ⌘↩ sends it to the session's terminal.
struct NotchClaudeSessionComposer: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    let size: CGFloat
    @ObservedObject private var service = ClaudeSessionService.shared
    @FocusState private var focused: Bool
    @State private var sending = false
    @State private var problem: String?

    private var draft: Binding<String> {
        Binding(get: { service.drafts[session.id] ?? "" }, set: { service.drafts[session.id] = $0 })
    }

    private var unavailable: String? {
        if session.phase.isWaiting { return text.replyWhileWaiting }
        if session.waitingFor != nil { return text.replyWhileDialog }
        if !session.host.acceptsReplies { return text.replyUnavailable }
        return nil
    }

    private var canSend: Bool {
        unavailable == nil && !sending && NotchClaudeSessionsSupport.reply(draft.wrappedValue) != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .bottom, spacing: 10) {
                ZStack(alignment: .topLeading) {
                    if draft.wrappedValue.isEmpty {
                        Text(unavailable ?? text.replyPlaceholder)
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
                        .accessibilityLabel(text.replyPlaceholder)
                }
                // About three lines; the editor scrolls past that.
                .frame(height: size * 4)
                .padding(10)
                .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .strokeBorder(.white.opacity(focused ? 0.3 : 0.08), lineWidth: 1)
                }
                Button(action: send) {
                    Group {
                        if sending { ProgressView().controlSize(.small) } else {
                            Image(systemName: "arrow.up").font(.system(size: 15, weight: .bold))
                        }
                    }
                    .foregroundStyle(canSend ? Color.black.opacity(0.85) : .white.opacity(0.35))
                    .frame(width: 38, height: 38)
                    .background(canSend ? AgentProvider.claude.tint : .white.opacity(0.08), in: Circle())
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
                Text(text.sendShortcut).foregroundStyle(.tertiary)
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
            switch failure {
            case nil: break
            case .waiting?: problem = text.sendWhileWaiting
            case .unreachable?: problem = text.sendFailed
            }
        }
    }
}
