// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// A Clawd fitted to a box, moving unless Reduce Motion is on. It is
/// decoration: what it stands for is said by the text beside it.
struct NotchClawd: View {
    let costume: ClawdCostume
    var motion = NotchClawdMotion.rest
    var seed: UInt64 = 0
    var cheer: Date? = nil
    var bubbles = true
    let size: CGSize
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        NotchClawdBridge(costume: costume, motion: motion, seed: seed, cheer: cheer, still: reduceMotion, bubbles: bubbles)
            .frame(width: size.width, height: size.height)
            .accessibilityHidden(true)
            .allowsHitTesting(false)
    }
}

private struct NotchClawdBridge: NSViewRepresentable {
    let costume: ClawdCostume
    let motion: NotchClawdMotion
    let seed: UInt64
    let cheer: Date?
    let still: Bool
    let bubbles: Bool

    func makeNSView(context: Context) -> NotchClawdView { NotchClawdView() }
    func updateNSView(_ view: NotchClawdView, context: Context) {
        view.configure(costume: costume, motion: motion, seed: seed, cheer: cheer, still: still, bubbles: bubbles)
    }
    static func dismantleNSView(_ view: NotchClawdView, coordinator: ()) { view.stop() }
}

/// A Clawd standing still, drawn as a picture: the wardrobe's tiles show
/// dozens at once and none of them moves.
struct NotchClawdPicture: View {
    let costume: ClawdCostume
    var pose = NotchClawdPose()
    let size: CGSize
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        let fit = NotchClawdSprite.layout(box: size, scale: displayScale)
        ZStack(alignment: .topLeading) {
            if let image = NotchClawdSprite.image(costume, pose) {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .interpolation(.none)
                    .frame(width: CGFloat(NotchClawdSprite.width) * fit.pixel,
                           height: CGFloat(NotchClawdSprite.height) * fit.pixel)
                    .offset(x: fit.origin.x, y: fit.origin.y)
            }
        }
        .frame(width: size.width, height: size.height, alignment: .topLeading)
        .accessibilityHidden(true)
    }
}

/// A session's Clawd in the costume it wears, doing what the session does.
/// With Clawds turned off, the phase's badge takes its place.
struct NotchClaudeSessionAvatar: View {
    let session: ClaudeSession
    var size = CGSize(width: 32, height: 30)
    /// The badge's size when Clawds are off.
    var badge: CGFloat = 26
    @ObservedObject private var service = ClaudeSessionService.shared
    @AppStorage(DefaultsKey.notchSessionsBuddies) private var buddies = true

    var body: some View {
        if buddies {
            NotchClawd(costume: service.costume(for: session),
                       motion: NotchClaudeSessionsSupport.motion(session, now: Date()),
                       seed: ClawdCostume.seed(session.id), cheer: NotchClaudeSessionsSupport.cheer(session), size: size)
        } else {
            NotchClaudeSessionBadge(phase: session.phase, size: badge)
        }
    }
}

/// A subagent's Clawd: at work while it runs, faded and smiling once done.
/// With Clawds turned off, `fallback` shows instead.
struct NotchClaudeSubagentAvatar<Fallback: View>: View {
    let agent: ClaudeSubagent
    let session: ClaudeSession
    var size = CGSize(width: 18, height: 14)
    @ViewBuilder let fallback: () -> Fallback
    @ObservedObject private var service = ClaudeSessionService.shared
    @AppStorage(DefaultsKey.notchSessionsBuddies) private var buddies = true

    var body: some View {
        if buddies {
            NotchClawd(costume: service.costume(for: agent, in: session),
                       motion: NotchClaudeSessionsSupport.motion(agent),
                       seed: ClawdCostume.seed(NotchClaudeSessionsSupport.costumeKey(agent, in: session)), size: size)
                .opacity(agent.isRunning ? 1 : 0.5)
        } else {
            fallback()
        }
    }
}

/// A session's Clawd as a button that opens its wardrobe. A small shirt
/// shows on it under the pointer, so it is found without a word on screen.
/// With Clawds turned off, the phase's badge stays as it was.
struct NotchClawdDressButton: View {
    let session: ClaudeSession
    let name: String
    let text: NotchClaudeSessionStrings
    var size = CGSize(width: 40, height: 36)
    var badge: CGFloat = 30
    @AppStorage(DefaultsKey.notchSessionsBuddies) private var buddies = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovered = false

    var body: some View {
        if buddies {
            Button { ClaudeSessionService.shared.dress(session.id) } label: {
                NotchClaudeSessionAvatar(session: session, size: size, badge: badge)
                    .overlay(alignment: .bottomTrailing) {
                        Image(systemName: "tshirt.fill")
                            .font(.system(size: 7.5, weight: .bold))
                            .foregroundStyle(.black.opacity(0.85))
                            .frame(width: 15, height: 15)
                            .background(.white.opacity(0.92), in: Circle())
                            .offset(x: 4, y: 3)
                            .opacity(hovered ? 1 : 0)
                            .scaleEffect(hovered || reduceMotion ? 1 : 0.6, anchor: .bottomTrailing)
                    }
                    .contentShape(Rectangle())
            }
            // No lift: a pixel drawing scaled by a fraction loses its edges.
            .buttonStyle(NotchButtonStyle(cornerRadius: 10, lifts: false))
            .onHover { hovering in
                withAnimation(reduceMotion ? nil : .spring(response: 0.25, dampingFraction: 0.7)) { hovered = hovering }
            }
            .help(text.dressUp)
            .accessibilityLabel(text.dressUp(name))
        } else {
            NotchClaudeSessionBadge(phase: session.phase, size: badge)
        }
    }
}

/// A session's Clawd in a notice about it: hopping while the session waits,
/// cheering once its work is done. The strip has no room above it, so no
/// bubble shows; the notice's words say what it wants.
struct NotchClaudeNoticeClawd: View {
    let sessionID: String
    let symbol: String
    let size: CGSize
    @ObservedObject private var service = ClaudeSessionService.shared

    var body: some View {
        let session = service.sessions.first { $0.id == sessionID }
        NotchClawd(costume: ClawdCostume.resolved(key: sessionID, kept: service.costumes[sessionID]),
                   motion: NotchClaudeSessionsSupport.noticeMotion(symbol: symbol), seed: ClawdCostume.seed(sessionID),
                   cheer: session.flatMap(NotchClaudeSessionsSupport.cheer), bubbles: false, size: size)
    }
}
