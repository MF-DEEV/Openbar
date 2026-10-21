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
    @AppStorage(DefaultsKey.notchSessionsContentPreview) private var previews = false
    @State private var openFailed = false

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    header(now: context.date)
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
        .notchBand(leading: {
            NotchBandBackButton(title: session.project) { service.focus(nil) }
        }, trailing: {
            openButton
        })
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
                Text(details)
                    .font(.system(size: 10.5))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
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

    private var openButton: some View {
        Button {
            service.openHost(session) { found in
                openFailed = !found
                if !found {
                    DispatchQueue.main.asyncAfter(deadline: .now() + 3) { openFailed = false }
                }
            }
        } label: {
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
            ForEach(session.subagents.suffix(4)) { agent in
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
        }
    }

    @ViewBuilder private var lastReply: some View {
        if let reply = session.lastText {
            VStack(alignment: .leading, spacing: 4) {
                sectionTitle(text.lastReply)
                Text(reply)
                    .font(.system(size: 11))
                    .foregroundStyle(.white.opacity(0.8))
                    .lineLimit(4)
                    .textSelection(.enabled)
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
