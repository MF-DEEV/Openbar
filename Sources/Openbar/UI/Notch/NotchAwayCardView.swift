// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// "While you were away" (#215): a tile per session that changed, opening
/// it on a tap. The island's top row holds how long the person was gone, a
/// tally, and a close button whose ring runs down to the card closing itself.
struct NotchAwayCardView: View {
    let size: CGSize
    let text: NotchClaudeSessionStrings
    @ObservedObject private var card = NotchAwayCard.shared
    @ObservedObject private var service = ClaudeSessionService.shared

    var body: some View {
        let tiles = card.lines.compactMap { line in service.sessions.first { $0.id == line.id }.map { (line, $0) } }
        // A tile is a third of the card wide; one or two sit in the middle.
        let width = max(80, (size.width - 12) / 3)
        ScrollView {
            LazyVGrid(columns: Array(repeating: GridItem(.fixed(width), spacing: 6), count: max(1, min(3, tiles.count))),
                      spacing: 6) {
                ForEach(Array(tiles.enumerated()), id: \.element.0.id) { index, tile in
                    NotchAwayTile(line: tile.0, session: tile.1, name: service.displayName(tile.1), index: index, text: text)
                }
            }
            .padding(.bottom, 4)
        }
        .scrollIndicators(.automatic)
        .onHover { card.held = $0 }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(text.awayCard)
        .notchBand(leading: { NotchAwayHeader(text: text) }, trailing: { NotchAwayClose(text: text) })
    }
}

extension NotchAway.Outcome {
    var tint: Color {
        switch self {
        case .waiting: return ClaudeSessionPhase.waitingPermission.tint
        case .error: return Color(red: 1.0, green: 0.37, blue: 0.34)
        case .limited: return ClaudeSessionPhase.limited.tint
        case .finished: return NotchSessionProgress.finishedTint
        case .working: return .white.opacity(0.7)
        }
    }

    var symbol: String {
        switch self {
        case .waiting: return "hand.raised.fill"
        case .error: return "xmark.circle.fill"
        case .limited: return NotchClaudeSessionsSupport.symbol(.limited)
        case .finished: return "checkmark.circle.fill"
        case .working: return "ellipsis"
        }
    }

    func label(_ text: NotchClaudeSessionStrings) -> String {
        switch self {
        case .waiting: return text.awayWaiting
        case .error: return text.awayError
        case .limited: return text.awayLimited
        case .finished: return text.awayFinished
        case .working: return text.awayWorking
        }
    }
}

private struct NotchAwayTile: View {
    let line: NotchAway.Line
    let session: ClaudeSession
    let name: String
    let index: Int
    let text: NotchClaudeSessionStrings
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false
    @State private var badge = false

    var body: some View {
        Button { NotchAwayCard.shared.open(session.id) } label: {
            VStack(spacing: 6) {
                NotchClaudeSessionAvatar(session: session, size: CGSize(width: 40, height: 36), badge: 30)
                    .overlay(alignment: .bottomTrailing) {
                        Image(systemName: line.outcome.symbol)
                            .font(.system(size: 10, weight: .bold))
                            .foregroundStyle(line.outcome.tint)
                            .frame(width: 19, height: 19)
                            .background(Circle().fill(.black))
                            .offset(x: 6, y: 4)
                            .scaleEffect(badge ? 1 : 0)
                    }
                Text(name)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.95))
                    .lineLimit(1)
                    .truncationMode(.tail)
                stats
            }
            .padding(.top, 12)
            .padding(.bottom, 9)
            .padding(.horizontal, 6)
            .frame(maxWidth: .infinity)
            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 16))
        .modifier(NotchControlSurface(cornerRadius: 16))
        .overlay {
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .strokeBorder(line.outcome.tint.opacity(0.22), lineWidth: 1)
                .allowsHitTesting(false)
        }
        .help(session.lastText ?? "")
        .accessibilityLabel("\(name), \(line.outcome.label(text))")
        .opacity(shown ? 1 : 0)
        .offset(y: shown ? 0 : 10)
        .scaleEffect(shown ? 1 : 0.96)
        .onAppear {
            guard !reduceMotion else { shown = true; badge = true; return }
            withAnimation(.spring(response: 0.38, dampingFraction: 0.78).delay(0.08 + Double(index) * 0.05)) { shown = true }
            withAnimation(.spring(response: 0.4, dampingFraction: 0.5).delay(0.35 + Double(index) * 0.05)) { badge = true }
        }
    }

    @ViewBuilder private var stats: some View {
        if line.files > 0 || line.tasks != nil {
            VStack(spacing: 4) {
                if line.files > 0 {
                  HStack(spacing: 4) {
                    HStack(spacing: 2) {
                        Image(systemName: "doc").font(.system(size: 8.5, weight: .semibold))
                        Text("\(line.files)")
                    }
                    .foregroundStyle(.white.opacity(0.6))
                    .accessibilityLabel(text.awayFiles(line.files))
                    if line.added > 0 { Text("+\(line.added)").foregroundStyle(NotchSessionProgress.finishedTint) }
                    if line.removed > 0 { Text("−\(line.removed)").foregroundStyle(ClaudeSessionPhase.limited.tint) }
                  }
                }
                if let tasks = line.tasks { NotchAwayTasks(tasks: tasks, tint: session.provider.tint, text: text) }
            }
            .font(.system(size: 10, weight: .semibold))
            .monospacedDigit()
            .lineLimit(1)
            .minimumScaleFactor(0.8)
        }
    }
}

/// Tasks ticked while away: the old share dim, the new part lit one by one.
private struct NotchAwayTasks: View {
    let tasks: NotchAway.Line.Tasks
    let tint: Color
    let text: NotchClaudeSessionStrings
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var lit = false

    var body: some View {
        let bars = min(tasks.total, 12)
        let scale = Double(bars) / Double(max(1, tasks.total))
        let before = Int((Double(tasks.from) * scale).rounded())
        let after = Int((Double(tasks.to) * scale).rounded())
        HStack(spacing: 4) {
            HStack(spacing: 1.5) {
                ForEach(0..<bars, id: \.self) { index in
                    let new = index >= before && index < after
                    RoundedRectangle(cornerRadius: 1)
                        .fill(index < before ? tint.opacity(0.45) : new && lit ? tint : .white.opacity(0.14))
                        .frame(width: 3, height: 9)
                        .scaleEffect(y: new && !lit ? 0.2 : 1, anchor: .bottom)
                        .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.55)
                            .delay(0.4 + Double(index - before) * 0.05), value: lit)
                }
            }
            Text("\(tasks.to)/\(tasks.total)").foregroundStyle(.white.opacity(0.7))
        }
        .onAppear { lit = true }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text.tasks(done: tasks.to, total: tasks.total))
    }
}

private struct NotchAwayHeader: View {
    let text: NotchClaudeSessionStrings
    @ObservedObject private var card = NotchAwayCard.shared

    var body: some View {
        let minutes = max(1, Int(((card.since.map { Date().timeIntervalSince($0) }) ?? 60) / 60))
        let tally: [(NotchAway.Outcome, Int)] = ([.waiting, .error, .limited, .finished] as [NotchAway.Outcome]).compactMap { outcome in
            let n = card.lines.filter { $0.outcome == outcome }.count
            return n > 0 ? (outcome, n) : nil
        }
        HStack(spacing: 7) {
            Image(systemName: "moon.stars").font(.system(size: 11, weight: .semibold)).foregroundStyle(.white.opacity(0.55))
            Text(text.awayFor(minutes: minutes))
                .font(.system(size: 12, weight: .semibold)).monospacedDigit().foregroundStyle(.white.opacity(0.92))
            ForEach(tally, id: \.0) { outcome, n in
                HStack(spacing: 3) {
                    Image(systemName: outcome.symbol).font(.system(size: 9, weight: .bold))
                    Text("\(n)").font(.system(size: 11, weight: .semibold)).monospacedDigit()
                }
                .foregroundStyle(outcome.tint)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(n) \(outcome.label(text))")
            }
        }
        .padding(.horizontal, 7)
        .frame(height: 28)
    }
}

private struct NotchAwayClose: View {
    let text: NotchClaudeSessionStrings
    @ObservedObject private var card = NotchAwayCard.shared

    var body: some View {
        Button { card.close() } label: {
            ZStack {
                Circle().fill(.white.opacity(0.08))
                Circle()
                    .trim(from: 0, to: card.left / NotchAwayCard.closeAfter)
                    .stroke(.white.opacity(0.5), style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                    .padding(1)
                Image(systemName: "xmark").font(.system(size: 9, weight: .bold)).foregroundStyle(.white.opacity(0.8))
            }
            .frame(width: 22, height: 22)
            .contentShape(Circle())
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 11))
        .help(text.awayClose)
        .accessibilityLabel(text.awayClose)
    }
}
