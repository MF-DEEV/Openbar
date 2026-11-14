// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// How a session's task list shows on its row (#215).
enum NotchSessionProgressStyle: String, CaseIterable {
    case ring, dots, bar
}

enum NotchSessionProgress {
    static let finishedTint = Color(red: 0.36, green: 0.84, blue: 0.52)

    /// "Opus" from "claude-opus-5-5", "GPT-5.5" from "gpt-5.5-codex": the family only.
    static func shortModel(_ session: ClaudeSession) -> String? {
        guard let raw = session.model, !raw.hasPrefix("<"), raw != "inherit" else { return nil }
        let full = AgentPricing.displayName(raw)
        guard !full.isEmpty else { return nil }
        if raw.hasPrefix("claude-") { return full.split(separator: " ").first.map(String.init) ?? full }
        for suffix in [" Codex", "-codex", " codex"] where full.hasSuffix(suffix) {
            return String(full.dropLast(suffix.count))
        }
        return full
    }

    /// 1 to 4 bars for the reasoning level, none when the agent names none.
    static func effortLevel(_ effort: String?) -> Int {
        switch effort?.lowercased() {
        case "minimal", "low": return 1
        case "medium": return 2
        case "high": return 3
        case "xhigh", "max": return 4
        default: return 0
        }
    }

    /// The list as items: the agent's own, or made up from the counts.
    static func items(_ tasks: ClaudeTaskProgress) -> [ClaudeTaskItem] {
        if !tasks.items.isEmpty { return tasks.items }
        return (0..<tasks.total).map { index in
            ClaudeTaskItem(title: index == tasks.done ? tasks.current ?? "" : "",
                           state: index < tasks.done ? .done : index == tasks.done ? .current : .pending)
        }
    }
}

/// A ring round the row's Clawd, filled to the share of tasks done.
struct NotchTaskRing: View {
    let tasks: ClaudeTaskProgress
    let tint: Color
    var lineWidth: CGFloat = 2
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let done = tasks.finished
        ZStack {
            Circle().stroke(.white.opacity(0.13), lineWidth: lineWidth)
            Circle()
                .trim(from: 0, to: min(1, tasks.fraction))
                .stroke(done ? NotchSessionProgress.finishedTint : tint,
                        style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .shadow(color: done ? NotchSessionProgress.finishedTint.opacity(0.6) : .clear, radius: done ? 3 : 0)
        }
        .animation(reduceMotion ? nil : .spring(response: 0.55, dampingFraction: 0.8), value: tasks.done)
        .animation(reduceMotion ? nil : .spring(response: 0.4, dampingFraction: 0.5), value: done)
        .accessibilityHidden(true)
        .allowsHitTesting(false)
    }
}

/// "4/9" on the row's first line, the number rolling as it moves on; a check
/// once the list is done; or a dot a task with the dots style.
struct NotchTaskCount: View {
    let tasks: ClaudeTaskProgress
    let tint: Color
    let style: NotchSessionProgressStyle
    let text: NotchClaudeSessionStrings
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Group {
            if style == .dots {
                dots
            } else if tasks.finished {
                Image(systemName: "checkmark")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(NotchSessionProgress.finishedTint)
                    .transition(.scale.combined(with: .opacity))
            } else {
                HStack(spacing: 0) {
                    Text("\(tasks.done)")
                        .foregroundStyle(.white.opacity(0.82))
                        .contentTransition(reduceMotion ? .identity : .numericText(value: Double(tasks.done)))
                    Text("/\(tasks.total)").foregroundStyle(.white.opacity(0.4))
                }
                .font(.system(size: 10.5, weight: .semibold))
                .monospacedDigit()
            }
        }
        .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.75), value: tasks.done)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text.tasks(done: tasks.done, total: tasks.total))
    }

    private var dots: some View {
        let items = NotchSessionProgress.items(tasks)
        return HStack(spacing: 2.5) {
            ForEach(Array(items.prefix(12).enumerated()), id: \.offset) { _, item in
                NotchTaskDot(state: item.state, tint: tint)
            }
            if items.count > 12 {
                Text("+\(items.count - 12)").font(.system(size: 9)).foregroundStyle(.white.opacity(0.45))
            }
        }
    }
}

private struct NotchTaskDot: View {
    let state: ClaudeTaskItem.State
    let tint: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var breathe = false

    var body: some View {
        Circle()
            .fill(state == .pending ? Color.white.opacity(0.16) : tint)
            .frame(width: 5, height: 5)
            .scaleEffect(state == .current ? 1.25 : 1)
            .opacity(state == .current && breathe ? 0.35 : 1)
            .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.55), value: state)
            .onAppear { startBreathing() }
            .onChange(of: state) { _, _ in startBreathing() }
    }

    private func startBreathing() {
        guard state == .current, !reduceMotion else { breathe = false; return }
        withAnimation(.easeInOut(duration: 0.7).repeatForever(autoreverses: true)) { breathe = true }
    }
}

/// A 2 pt bar along the row's bottom edge.
struct NotchTaskBar: View {
    let tasks: ClaudeTaskProgress
    let tint: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(.white.opacity(0.07))
                Capsule()
                    .fill(tasks.finished ? NotchSessionProgress.finishedTint : tint)
                    .frame(width: proxy.size.width * min(1, tasks.fraction))
                    .shadow(color: tint.opacity(0.7), radius: 3)
            }
        }
        .frame(height: 2)
        .padding(.horizontal, 12)
        .animation(reduceMotion ? nil : .spring(response: 0.55, dampingFraction: 0.85), value: tasks.done)
        .accessibilityHidden(true)
        .allowsHitTesting(false)
    }
}

/// Which model a session runs on: a quiet chip whose name rolls over on a
/// switch, and one to four bars for how hard it thinks.
struct NotchModelChip: View {
    let session: ClaudeSession
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        if let name = NotchSessionProgress.shortModel(session) {
            let level = NotchSessionProgress.effortLevel(session.effort)
            HStack(spacing: 4) {
                Text(name)
                    .id(name)
                    .transition(reduceMotion ? .identity
                                : .asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity),
                                              removal: .move(edge: .top).combined(with: .opacity)))
                if level > 0 { NotchEffortBars(level: level) }
            }
            .font(.system(size: 9.5, weight: .semibold))
            .foregroundStyle(.white.opacity(0.72))
            .padding(.leading, 6)
            .padding(.trailing, 5)
            .padding(.vertical, 1)
            .background(Capsule().fill(.white.opacity(0.08)))
            .clipShape(Capsule())
            .fixedSize()
            .animation(reduceMotion ? nil : .spring(response: 0.38, dampingFraction: 0.8), value: name)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel([name, session.effort].compactMap { $0 }.joined(separator: ", "))
        }
    }
}

/// One to four rising bars, like a signal reading.
struct NotchEffortBars: View {
    let level: Int

    var body: some View {
        HStack(alignment: .bottom, spacing: 1) {
            ForEach(0..<4, id: \.self) { index in
                RoundedRectangle(cornerRadius: 0.6)
                    .fill(.white.opacity(index < level ? 0.9 : 0.22))
                    .frame(width: 2, height: CGFloat(2 + index * 2))
            }
        }
        .frame(height: 8, alignment: .bottom)
        .accessibilityHidden(true)
    }
}

/// The list on the session's page: one segment a task above, then the last
/// one done, the one under way and the next few, done ones struck through.
struct NotchTaskChecklist: View {
    let tasks: ClaudeTaskProgress
    let tint: Color
    let text: NotchClaudeSessionStrings
    var rows = 4
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let items = NotchSessionProgress.items(tasks)
        let at = min(tasks.done, max(0, items.count - 1))
        let from = items.count > rows ? max(0, min(at - 1, items.count - rows)) : 0
        let shown = Array(items.enumerated()).dropFirst(from).prefix(rows)
        let done = tasks.finished
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 8) {
                HStack(spacing: 3) {
                    ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                        NotchTaskSegment(state: item.state, tint: done ? NotchSessionProgress.finishedTint : tint)
                    }
                }
                Group {
                    if done {
                        Image(systemName: "checkmark").font(.system(size: 9.5, weight: .bold))
                            .foregroundStyle(NotchSessionProgress.finishedTint)
                    } else {
                        Text("\(tasks.done)/\(tasks.total)")
                            .contentTransition(reduceMotion ? .identity : .numericText(value: Double(tasks.done)))
                            .foregroundStyle(.white.opacity(0.8))
                    }
                }
                .font(.system(size: 10.5, weight: .semibold))
                .monospacedDigit()
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(text.tasks(done: tasks.done, total: tasks.total))
            if items.contains(where: { !$0.title.isEmpty }) {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(Array(shown), id: \.offset) { _, item in
                        NotchTaskRow(item: item, tint: tint)
                    }
                }
            }
        }
        .animation(reduceMotion ? nil : .spring(response: 0.4, dampingFraction: 0.85), value: tasks.done)
    }
}

private struct NotchTaskSegment: View {
    let state: ClaudeTaskItem.State
    let tint: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shimmer = false

    var body: some View {
        Capsule()
            .fill(.white.opacity(0.1))
            .overlay(alignment: .leading) {
                GeometryReader { proxy in
                    Capsule()
                        .fill(tint)
                        .frame(width: state == .pending ? 0 : proxy.size.width)
                        .opacity(state == .current ? (shimmer ? 0.35 : 0.9) : 1)
                }
            }
            .frame(height: 4)
            .frame(maxWidth: .infinity)
            .onAppear { animate() }
            .onChange(of: state) { _, _ in animate() }
    }

    private func animate() {
        guard state == .current, !reduceMotion else { shimmer = false; return }
        withAnimation(.easeInOut(duration: 0.8).repeatForever(autoreverses: true)) { shimmer = true }
    }
}

private struct NotchTaskRow: View {
    let item: ClaudeTaskItem
    let tint: Color

    var body: some View {
        HStack(spacing: 8) {
            ZStack {
                switch item.state {
                case .done:
                    Circle().fill(tint)
                    Image(systemName: "checkmark").font(.system(size: 7, weight: .heavy)).foregroundStyle(.black)
                        .transition(.scale.combined(with: .opacity))
                case .current:
                    NotchTaskSpinner(tint: tint)
                case .pending:
                    Circle().strokeBorder(.white.opacity(0.2), lineWidth: 1.5)
                }
            }
            .frame(width: 13, height: 13)
            Text(item.title)
                .strikethrough(item.state == .done, color: .white.opacity(0.25))
                .lineLimit(1)
                .truncationMode(.tail)
        }
        .font(.system(size: 11, weight: item.state == .current ? .medium : .regular))
        .foregroundStyle(item.state == .done ? .white.opacity(0.42) : item.state == .current ? .white.opacity(0.95) : .white.opacity(0.3))
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 7, style: .continuous)
            .fill(.white.opacity(item.state == .current ? 0.07 : 0)))
    }
}

private struct NotchTaskSpinner: View {
    let tint: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var turn = false

    var body: some View {
        Circle()
            .trim(from: 0, to: 0.7)
            .stroke(tint, style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
            .background(Circle().stroke(tint.opacity(0.25), lineWidth: 1.5))
            .frame(width: 11, height: 11)
            .rotationEffect(.degrees(turn ? 360 : 0))
            .onAppear {
                guard !reduceMotion else { return }
                withAnimation(.linear(duration: 0.9).repeatForever(autoreverses: false)) { turn = true }
            }
    }
}
