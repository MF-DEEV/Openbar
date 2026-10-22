// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// Where a session's Clawd is dressed: the Clawd itself on the left, doing
/// what its session does, and on the right a row for each part of its
/// costume. A choice dresses it at once, and it hops for it.
struct NotchClawdWardrobeView: View {
    let session: ClaudeSession
    let size: CGSize
    @ObservedObject private var service = ClaudeSessionService.shared
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// When the costume last changed here, which the Clawd hops for.
    @State private var changed: Date?

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }
    private var costume: ClawdCostume { service.costume(for: session) }
    private var dressedByHand: Bool { service.costumes[session.id] != nil }
    /// The stage grows with an enlarged island, up to a point.
    private var stageWidth: CGFloat { min(230, max(150, size.width * 0.3)) }
    private var stageHeight: CGFloat { stageWidth * 0.92 }

    var body: some View {
        VStack(spacing: 8) {
            if session.phase.isWaiting { waitingBar }
            HStack(alignment: .top, spacing: 14) {
                stage
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        shells
                        part(text.hatTitle, chosen: text.name(costume.hat), parts: ClawdHat.allCases,
                             name: text.name, selected: costume.hat) { dressed(\.hat, $0) }
                        part(text.eyewearTitle, chosen: text.name(costume.eyewear), parts: ClawdEyewear.allCases,
                             name: text.name, selected: costume.eyewear) { dressed(\.eyewear, $0) }
                        part(text.extrasTitle, chosen: text.name(costume.extra), parts: ClawdExtra.allCases,
                             name: text.name, selected: costume.extra) { dressed(\.extra, $0) }
                    }
                    .padding(.bottom, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .scrollIndicators(.automatic)
            }
        }
        .notchBand(leading: {
            NotchBandBackButton(title: text.wardrobe) { service.dress(nil) }
        }, trailing: {
            HStack(spacing: 2) {
                surpriseButton
                resetButton
            }
        })
    }

    // MARK: Stage

    private var stage: some View {
        // The whole canvas fits the card, so a tall hat, a raised arm, a cup
        // held out and the highest jump all stay inside it.
        let margin: CGFloat = 10
        let pixel = min((stageWidth - margin * 2) / CGFloat(NotchClawdSprite.width),
                        (stageHeight - margin * 2) / CGFloat(NotchClawdSprite.height))
        let figure = CGSize(width: pixel * CGFloat(NotchClawdSprite.frameColumns.count),
                            height: pixel * CGFloat(NotchClawdSprite.frameRows.count))
        return VStack(spacing: 8) {
            ZStack(alignment: .bottom) {
                // The floor it stands on, a soft light under its feet.
                Ellipse()
                    .fill(.white.opacity(0.07))
                    .frame(width: figure.width * 0.9, height: pixel * 2)
                    .offset(y: pixel)
                NotchClawd(costume: costume, motion: NotchClaudeSessionsSupport.motion(session, now: Date()),
                           seed: ClawdCostume.seed(session.id), since: session.phaseSince,
                           cheer: changed ?? NotchClaudeSessionsSupport.cheer(session),
                           size: figure)
            }
            .padding(.bottom, margin + pixel)
            .frame(width: stageWidth, height: stageHeight, alignment: .bottom)
            .modifier(NotchControlSurface(cornerRadius: 18, interactive: false))
            .accessibilityElement()
            .accessibilityLabel(outfit)
            Text(service.displayName(session))
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.white.opacity(0.95))
                .lineLimit(1)
                .truncationMode(.tail)
            NotchClaudeProjectTag(project: session.project)
        }
        .frame(width: stageWidth)
    }

    /// The costume read aloud, part by part.
    private var outfit: String {
        [text.name(costume.shell), costume.hat == .none ? nil : text.name(costume.hat),
         costume.eyewear == .none ? nil : text.name(costume.eyewear),
         costume.extra == .none ? nil : text.name(costume.extra)]
            .compactMap { $0 }.joined(separator: ", ")
    }

    /// The session asked for something while it was being dressed.
    private var waitingBar: some View {
        Button { service.dress(nil) } label: {
            HStack(spacing: 6) {
                Image(systemName: NotchClaudeSessionsSupport.symbol(session.phase))
                    .font(.system(size: 11, weight: .semibold))
                Text(text.phase(session.phase))
                    .font(.system(size: 12, weight: .semibold))
                Spacer(minLength: 6)
                Text(text.goToSession)
                    .font(.system(size: 12, weight: .medium))
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .bold))
            }
            .foregroundStyle(session.phase.tint)
            .padding(.horizontal, 12)
            .frame(height: 30)
            .background(session.phase.tint.opacity(0.14), in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 15, lifts: false))
        .transition(.opacity)
    }

    // MARK: Parts

    private var shells: some View {
        VStack(alignment: .leading, spacing: 6) {
            heading(text.shellTitle, chosen: text.name(costume.shell))
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 26, maximum: 30), spacing: 3)], alignment: .leading, spacing: 3) {
                ForEach(ClawdShell.allCases) { shell in
                    let selected = shell == costume.shell
                    let color = shell.rgb
                    Button { dressed(\.shell, shell) } label: {
                        Circle()
                            .fill(Color(.sRGB, red: Double(color.red) / 255, green: Double(color.green) / 255,
                                        blue: Double(color.blue) / 255))
                            .frame(width: 20, height: 20)
                            .padding(3)
                            .overlay { Circle().strokeBorder(.white.opacity(selected ? 0.9 : 0), lineWidth: 2) }
                            .frame(width: 28, height: 28)
                            .contentShape(Circle())
                    }
                    .buttonStyle(.plain)
                    .help(text.name(shell))
                    .accessibilityLabel(text.name(shell))
                    .accessibilityAddTraits(selected ? .isSelected : [])
                    .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: selected)
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(text.shellTitle)
    }

    /// A row of tiles for one part: nothing, then each piece on a Clawd in
    /// the shell it wears, with nothing else on, so the piece reads.
    private func part<Part: Hashable & Identifiable>(_ title: String, chosen: String, parts: [Part],
                                                     name: @escaping (Part) -> String, selected: Part,
                                                     choose: @escaping (Part) -> Void) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            heading(title, chosen: chosen)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 46, maximum: 54), spacing: 6)], alignment: .leading, spacing: 6) {
                ForEach(parts) { piece in
                    tile(piece, isSelected: piece == selected, name: name(piece)) { choose(piece) }
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(title)
    }

    private func tile<Part>(_ piece: Part, isSelected: Bool, name: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            ZStack {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(.white.opacity(isSelected ? 0.13 : 0.05))
                if let outfit = wearing(piece) {
                    NotchClawdPicture(costume: outfit, size: CGSize(width: 34, height: 30))
                        .offset(y: 3)
                } else {
                    Image(systemName: "line.diagonal")
                        .font(.system(size: 15, weight: .regular))
                        .foregroundStyle(.white.opacity(0.4))
                }
            }
            .frame(height: 44)
            .overlay {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(.white.opacity(isSelected ? 0.85 : 0.07), lineWidth: isSelected ? 1.5 : 1)
            }
            .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 10, lifts: false))
        .help(name)
        .accessibilityLabel(name)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: isSelected)
    }

    /// The Clawd a tile shows: in this session's shell, wearing only the
    /// piece; nil for the tile that takes the part off.
    private func wearing<Part>(_ piece: Part) -> ClawdCostume? {
        var outfit = ClawdCostume(shell: costume.shell, hat: .none, eyewear: .none, extra: .none)
        switch piece {
        case let hat as ClawdHat where hat != .none: outfit.hat = hat
        case let eyewear as ClawdEyewear where eyewear != .none: outfit.eyewear = eyewear
        case let extra as ClawdExtra where extra != .none: outfit.extra = extra
        default: return nil
        }
        return outfit
    }

    private func heading(_ title: String, chosen: String) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.white.opacity(0.85))
            Spacer(minLength: 8)
            Text(chosen)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(.trailing, 4)
    }

    // MARK: Changing

    private func dressed<Value>(_ part: WritableKeyPath<ClawdCostume, Value>, _ value: Value) {
        var next = costume
        next[keyPath: part] = value
        guard next != costume else { return }
        wear(next)
    }

    private func wear(_ next: ClawdCostume?) {
        service.setCostume(next, for: session.id)
        changed = Date()
    }

    private var surpriseButton: some View {
        Button { wear(ClawdCostume.random(excluding: costume)) } label: {
            HStack(spacing: 5) {
                Image(systemName: "dice")
                    .font(.system(size: 11, weight: .semibold))
                Text(text.surpriseMe)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
            }
            .foregroundStyle(.white.opacity(0.9))
            .padding(.horizontal, 9)
            .frame(height: 28)
            .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 9))
        .help(text.surpriseMe)
    }

    private var resetButton: some View {
        Button { wear(nil) } label: {
            Image(systemName: "arrow.uturn.backward")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.white.opacity(0.9))
                .frame(width: 28, height: 28)
                .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 9))
        .disabled(!dressedByHand)
        .help(text.ownOutfit)
        .accessibilityLabel(text.ownOutfit)
    }
}
