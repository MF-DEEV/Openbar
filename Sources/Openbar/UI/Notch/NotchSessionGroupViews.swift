// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The folded groups, as stored: keys joined by new lines (#233).
enum NotchSessionGroupStore {
    static func keys(_ raw: String) -> [String] {
        raw.split(separator: "\n").map(String.init).filter { !$0.isEmpty }
    }

    static func raw(_ keys: [String]) -> String { keys.joined(separator: "\n") }
}

/// One list, by project, by agent: three marks in the band, one press each
/// (windows/src/sessions/SessionGroups.tsx).
struct NotchSessionGroupingSwitch: View {
    let text: NotchClaudeSessionStrings
    @AppStorage(DefaultsKey.notchSessionsGrouping) private var groupingRaw = NotchSessionGrouping.none.rawValue
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Namespace private var pill

    private var grouping: NotchSessionGrouping { NotchSessionGrouping(rawValue: groupingRaw) ?? .none }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(NotchSessionGrouping.allCases, id: \.self) { choice in
                let on = choice == grouping
                Button {
                    withAnimation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.8)) {
                        groupingRaw = choice.rawValue
                    }
                } label: {
                    symbol(choice)
                        .font(.system(size: 11, weight: .semibold))
                        .frame(width: 13, height: 13)
                        .foregroundStyle(.white.opacity(on ? 1 : 0.5))
                        .frame(width: 24, height: 22)
                        .background {
                            if on {
                                RoundedRectangle(cornerRadius: 7, style: .continuous)
                                    .fill(.white.opacity(0.18))
                                    .matchedGeometryEffect(id: "pill", in: pill)
                            }
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(NotchGroupingButtonStyle())
                .help(label(choice))
                .accessibilityLabel(label(choice))
                .accessibilityAddTraits(on ? [.isSelected] : [])
            }
        }
        .padding(2)
        .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(text.groupSessions)
    }

    @ViewBuilder private func symbol(_ choice: NotchSessionGrouping) -> some View {
        switch choice {
        case .none: Image(systemName: "list.bullet")
        case .project: Image(systemName: "folder")
        case .agent: SymbolImage(OpenbarSymbol.agentFace)
        }
    }

    private func label(_ choice: NotchSessionGrouping) -> String {
        switch choice {
        case .none: return text.groupNone
        case .project: return text.groupByProject
        case .agent: return text.groupByAgent
        }
    }
}

/// A press that gives a little, without the band buttons' wash.
private struct NotchGroupingButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.92 : 1)
            .animation(reduceMotion ? nil : .spring(response: 0.2, dampingFraction: 0.6), value: configuration.isPressed)
    }
}

/// A group's header: what it is, how many, whether someone waits, and the fold.
struct NotchSessionGroupHeader: View {
    let group: NotchSessionGroup
    let collapsed: Bool
    let text: NotchClaudeSessionStrings
    let toggle: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var tint: Color {
        if let provider = group.provider { return provider.tint }
        return NotchClaudeProjectTag.color(group.title)
    }

    private var waitingTint: Color { ClaudeSessionPhase.waitingPermission.tint }

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: 6) {
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(.white.opacity(0.4))
                    .rotationEffect(.degrees(collapsed ? 0 : 90))
                    .animation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.62), value: collapsed)
                mark
                Text(group.title)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(tint)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let remote = group.remote { NotchRemoteHostBadge(alias: remote) }
                Text("\(group.sessions.count)")
                    .font(.system(size: 9.5, weight: .semibold))
                    .monospacedDigit()
                    .foregroundStyle(.white.opacity(0.62))
                    .padding(.horizontal, 5)
                    .frame(minWidth: 16, minHeight: 15)
                    .background(.white.opacity(0.1), in: Capsule())
                Spacer(minLength: 4)
                if collapsed {
                    NotchSessionFaceStack(sessions: group.sessions)
                        .transition(reduceMotion ? .opacity : .opacity.combined(with: .offset(x: 6)))
                }
                if group.waiting > 0 {
                    HStack(spacing: 2) {
                        Image(systemName: "hand.raised.fill").font(.system(size: 8.5, weight: .semibold))
                        Text("\(group.waiting)").font(.system(size: 10, weight: .semibold)).monospacedDigit()
                    }
                    .foregroundStyle(waitingTint)
                    .padding(.horizontal, 6)
                    .frame(height: 16)
                    .background(waitingTint.opacity(0.16), in: Capsule())
                    .transition(reduceMotion ? .opacity : .scale(scale: 0.5).combined(with: .opacity))
                }
            }
            .padding(.leading, 4)
            .padding(.trailing, 6)
            .frame(maxWidth: .infinity, minHeight: 30, maxHeight: 30)
            .background {
                // Folded with someone inside waiting: the header keeps the amber the row had.
                if collapsed, group.waiting > 0 {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(waitingTint.opacity(0.07))
                        .overlay {
                            RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .strokeBorder(waitingTint.opacity(0.45), lineWidth: 1)
                        }
                }
            }
            .contentShape(Rectangle())
            .animation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.82), value: collapsed)
            .animation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.6), value: group.waiting > 0)
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 10, lifts: false))
        .accessibilityLabel(accessibilityText)
        .accessibilityValue(collapsed ? text.groupFolded : text.groupOpen)
        .accessibilityHint(collapsed ? text.groupOpenHint : text.groupFoldHint)
    }

    /// A project's first letter on its color, or the agent's own mark.
    @ViewBuilder private var mark: some View {
        if let provider = group.provider {
            NotchSessionAgentGlyph(provider: provider, size: 10, working: false)
                .frame(width: 16, height: 16)
                .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 5, style: .continuous))
                .accessibilityHidden(true)
        } else {
            Text(group.title.first.map { String($0).uppercased() } ?? "?")
                .font(.system(size: 9.5, weight: .bold))
                .foregroundStyle(tint)
                .frame(width: 16, height: 16)
                .background(tint.opacity(0.22), in: RoundedRectangle(cornerRadius: 5, style: .continuous))
                .accessibilityHidden(true)
        }
    }

    private var accessibilityText: String {
        var parts = [group.title, text.groupCount(group.sessions.count)]
        if let remote = group.remote { parts.append("On " + remote + ", over SSH") }
        if group.waiting > 0 { parts.append(text.groupWaiting(group.waiting)) }
        return parts.joined(separator: ", ")
    }
}

/// A folded group's sessions as small faces ringed in their phase's color.
struct NotchSessionFaceStack: View {
    let sessions: [ClaudeSession]

    var body: some View {
        let shown = Array(sessions.prefix(4))
        HStack(spacing: -4) {
            ForEach(shown) { session in
                NotchSessionAgentGlyph(provider: session.provider, size: 9, working: false)
                    .frame(width: 16, height: 16)
                    .background(Color(white: 0.067), in: Circle())
                    .overlay { Circle().strokeBorder(session.tint, lineWidth: 1.5) }
                    .background(Circle().fill(.black).padding(-1.5))
            }
            if sessions.count > shown.count {
                Text("+\(sessions.count - shown.count)")
                    .font(.system(size: 9.5, weight: .medium))
                    .monospacedDigit()
                    .foregroundStyle(.white.opacity(0.5))
                    .padding(.leading, 8)
            }
        }
        .accessibilityHidden(true)
    }
}

/// The row's branch, cut in the middle, with the worktree's name when the
/// branch doesn't carry it. The whole of it shows under the pointer.
struct NotchSessionBranchChip: View {
    let session: ClaudeSession
    var size: CGFloat = 10.5
    var maxWidth: CGFloat = 150

    var body: some View {
        if let label = NotchSessionGroups.branchLabel(session) {
            HStack(spacing: 3) {
                Image(systemName: label.inWorktree ? "folder" : "arrow.triangle.branch")
                    .font(.system(size: size - 1.5, weight: .semibold))
                    .imageScale(.small)
                if let worktree = label.worktree {
                    Text(worktree).lineLimit(1).truncationMode(.middle)
                    if label.branch != nil { Text("·").opacity(0.5) }
                }
                if let branch = label.branch {
                    Text(branch).lineLimit(1).truncationMode(.middle)
                }
            }
            .font(.system(size: size))
            .foregroundStyle(.white.opacity(0.5))
            .frame(maxWidth: maxWidth, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .help(label.full)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label.full)
        }
    }
}
