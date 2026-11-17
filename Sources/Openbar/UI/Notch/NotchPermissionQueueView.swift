// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// Every ask waiting on the person, on top of the Sessions list (#269):
/// how many, Allow all for the ones that look safe, and each ask on its own
/// row with its own buttons. A new ask joins at the foot.
struct NotchPermissionQueueView: View {
    let queue: NotchPermissionQueue
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private static let tint = ClaudeSessionPhase.waitingPermission.tint

    var body: some View {
        let allowAll = queue.allowAll
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: NotchClaudeSessionsSupport.symbol(.waitingPermission))
                    .font(.system(size: 11, weight: .semibold))
                Text(text.queueTitle(queue.count))
                    .font(.system(size: 12, weight: .semibold))
                    .contentTransition(.numericText())
                Spacer(minLength: 4)
                if !allowAll.isEmpty {
                    Button {
                        service.allowAll(queue)
                    } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "checkmark").font(.system(size: 10, weight: .bold))
                            Text(text.allowAll).font(.system(size: 11.5, weight: .semibold)).lineLimit(1)
                            Text("\(allowAll.count)")
                                .font(.system(size: 10.5, weight: .bold))
                                .monospacedDigit()
                                .contentTransition(.numericText())
                                .padding(.horizontal, 5)
                                .frame(minWidth: 16, minHeight: 16)
                                .background(.black.opacity(0.14), in: Capsule())
                        }
                        .foregroundStyle(Color.black.opacity(0.85))
                        .padding(.leading, 10)
                        .padding(.trailing, 6)
                        .frame(height: 28)
                        .background(NotchClaudeDecisionButton.allowColor, in: Capsule())
                        .contentShape(Capsule())
                    }
                    .buttonStyle(NotchButtonStyle(cornerRadius: 14))
                    .help(text.allowAllHelp(allowAll.count))
                    .accessibilityLabel("\(text.allowAll), \(allowAll.count)")
                    .transition(.opacity)
                }
            }
            .foregroundStyle(Self.tint)
            VStack(spacing: 4) {
                ForEach(queue.rows) { row in
                    NotchPermissionQueueRow(row: row, name: service.displayName(row.session), text: text)
                        .transition(reduceMotion ? .opacity
                                    : .asymmetric(insertion: .opacity.combined(with: .move(edge: .bottom)),
                                                  removal: .opacity.combined(with: .scale(scale: 0.97))))
                }
            }
            if queue.heldBack > 0, !allowAll.isEmpty {
                Text(text.queueHeldBack(queue.heldBack))
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
                    .padding(.leading, 2)
            }
        }
        .padding(8)
        .background(Self.tint.opacity(0.07), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .strokeBorder(Self.tint.opacity(0.4), lineWidth: 1)
                .allowsHitTesting(false)
        }
        .animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.86), value: queue.rows.map(\.id))
        .animation(reduceMotion ? nil : .easeOut(duration: 0.18), value: allowAll.count)
    }
}

/// One ask of the queue: whose, which tool, what it would run or touch, and
/// why Allow all leaves it out. Its buttons wait a moment after it appears,
/// so a click meant for the row that was there cannot answer it.
struct NotchPermissionQueueRow: View {
    let row: NotchPermissionQueue.Row
    let name: String
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var settling = true

    private var session: ClaudeSession { row.session }
    private var ask: ClaudePermission { row.ask }

    var body: some View {
        HStack(spacing: 8) {
            Button {
                service.focus(session.id, request: ask.requestID)
            } label: {
                HStack(spacing: 8) {
                    NotchClaudeSessionAvatar(session: session, size: CGSize(width: 24, height: 22), badge: 20, agentMark: true)
                        .frame(width: 26)
                    VStack(alignment: .leading, spacing: 1) {
                        HStack(spacing: 5) {
                            Text(name)
                                .font(.system(size: 11.5, weight: .semibold))
                                .foregroundStyle(.white.opacity(0.95))
                                .lineLimit(1)
                                .truncationMode(.tail)
                            Text(title)
                                .font(.system(size: 10.5, weight: .medium))
                                .foregroundStyle(.white.opacity(0.55))
                                .lineLimit(1)
                                .layoutPriority(1)
                            if let risk = text.risk(ask.risk) { NotchPermissionRiskTag(title: risk) }
                        }
                        if !row.line.isEmpty {
                            Text(verbatim: row.line)
                                .font(.system(size: 10.5, design: ask.tool == ClaudeToolSummary.planTool || ask.asksQuestions ? .default : .monospaced))
                                .foregroundStyle(.white.opacity(0.8))
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                    }
                    Spacer(minLength: 0)
                }
                .padding(.leading, 6)
                .frame(maxWidth: .infinity, minHeight: 40, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(NotchButtonStyle(cornerRadius: 10, lifts: false))
            .help(text.queueOpenHelp)
            .accessibilityLabel([name, title, text.risk(ask.risk), row.line].compactMap { $0 }.joined(separator: ", "))
            buttons.disabled(settling)
        }
        .padding(.trailing, 6)
        .background(.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .task(id: row.id) {
            settling = true
            try? await Task.sleep(for: .seconds(NotchClaudeSessionsSupport.settleDelay))
            settling = false
        }
    }

    private var title: String {
        if ask.asksQuestions { return text.question(for: session.provider) }
        if ask.tool == ClaudeToolSummary.planTool { return text.plan }
        return ask.tool
    }

    @ViewBuilder private var buttons: some View {
        HStack(spacing: 5) {
            if row.answersHere {
                NotchClaudeDecisionButton(title: text.deny, symbol: "xmark", role: .deny, compact: true) {
                    service.decide(session.id, request: ask.requestID, .deny)
                }
                if row.offersAlways {
                    NotchClaudeDecisionButton(title: text.alwaysAllow, symbol: "checkmark.seal", role: .secondary, compact: true) {
                        // Codex's Always lasts its turn, as on its card.
                        if session.provider == .codex {
                            service.decide(session.id, request: ask.requestID, .allow, autoAllowTurn: true)
                        } else {
                            service.decide(session.id, request: ask.requestID, .allowAlways)
                        }
                    }
                }
                NotchClaudeDecisionButton(title: text.allow, symbol: "checkmark", role: .allow, compact: true) {
                    service.decide(session.id, request: ask.requestID, .allow)
                }
            } else {
                // A question, a plan or a command to read whole: on its card.
                NotchClaudeDecisionButton(title: text.queueReview, symbol: "arrow.right", role: .secondary) {
                    service.focus(session.id, request: ask.requestID)
                }
            }
        }
    }
}

/// Why Allow all leaves an ask out, in a word or three.
struct NotchPermissionRiskTag: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.system(size: 9.5, weight: .semibold))
            .foregroundStyle(Color.orange)
            .lineLimit(1)
            .padding(.horizontal, 5)
            .padding(.vertical, 1.5)
            .background(Color.orange.opacity(0.16), in: Capsule())
            .fixedSize()
    }
}
