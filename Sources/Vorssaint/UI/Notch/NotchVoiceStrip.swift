// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

/// The microphone's marks: crossed-out headphones beside it when deafened,
/// red while muted and green when it opens again.
struct NotchVoiceMarks: View {
    let reading: VoiceChatReading
    let size: CGFloat
    let spacing: CGFloat
    /// Each mark's slot, so a notice measures the marks as they are drawn.
    var slot: CGFloat? = nil

    static let mutedTint = Color(nsColor: .systemRed)
    static let openTint = Color(nsColor: .systemGreen)

    static func tint(_ reading: VoiceChatReading?) -> Color {
        reading?.muted == false ? openTint : mutedTint
    }

    var body: some View {
        // A new reading is drawn anew, so each change pops in again.
        Marks(reading: reading, size: size, spacing: spacing, slot: slot)
            .id(reading)
            .accessibilityHidden(true)
    }

    private struct Marks: View {
        let reading: VoiceChatReading
        let size: CGFloat
        let spacing: CGFloat
        let slot: CGFloat?
        @Environment(\.accessibilityReduceMotion) private var reduceMotion
        @State private var appeared = false

        var body: some View {
            HStack(spacing: spacing) {
                if reading.deafened { mark(DiscordVoiceSupport.deafenedMark) }
                mark(reading.muted ? DiscordVoiceSupport.mutedSymbol : DiscordVoiceSupport.openSymbol)
            }
            .foregroundStyle(NotchVoiceMarks.tint(reading))
            // Each change pops in, as the Windows island draws it, unless motion is reduced.
            .scaleEffect(appeared || reduceMotion ? 1 : 0.65)
            .opacity(appeared || reduceMotion ? 1 : 0)
            .onAppear {
                guard !reduceMotion else { appeared = true; return }
                withAnimation(.easeOut(duration: 0.22)) { appeared = true }
            }
        }

        private func mark(_ name: String) -> some View {
            Image(systemName: name)
                .font(.system(size: size, weight: .medium))
                .frame(width: slot)
        }
    }
}

/// The closed island while the microphone is muted: its marks left of the
/// camera, and a red dot on the right that says it stays muted. Both open
/// Controls, where the microphone's tile is.
struct NotchVoiceStrip: View {
    @ObservedObject var service: NotchService
    /// Where the island draws it: its own strip as of the last update, or
    /// another display's when the island shows on every display.
    var displayGeometry: NotchGeometry? = nil
    @ObservedObject private var voice = DiscordVoiceService.shared
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        let reading = DiscordVoiceSupport.reading(voice.snapshot)
        // Unmuting clears the marks before the strip has left.
        NotchStripHold(reading, shows: reading.active) { strip($0) }
    }

    private func strip(_ reading: VoiceChatReading) -> some View {
        let geometry = displayGeometry ?? service.compactActivityGeometry
        let height = geometry.compactActivityContentHeight
        let wing = geometry.compactActivityWingWidth
        let iconSize = DiscordVoiceSupport.stripMarkSize(height: height, deafened: reading.deafened)
        return Button { service.openActivity(NotchCompactActivity.voiceChat.module) } label: {
            HStack(spacing: 0) {
                Group {
                    if wing >= 28 { NotchVoiceMarks(reading: reading, size: iconSize, spacing: DiscordVoiceSupport.stripMarkSpacing) }
                }
                .padding(.leading, geometry.compactActivityEdgeInset(boxHeight: iconSize, radius: iconSize / 2))
                .frame(width: wing, height: height, alignment: .leading)
                Color.clear.frame(width: geometry.compactActivityCameraGap)
                Circle()
                    .fill(NotchVoiceMarks.mutedTint)
                    .frame(width: DiscordVoiceSupport.dotSide, height: DiscordVoiceSupport.dotSide)
                    .shadow(color: NotchVoiceMarks.mutedTint.opacity(0.35), radius: 4)
                    .padding(.trailing, geometry.compactActivityEdgeInset(boxHeight: DiscordVoiceSupport.dotSide,
                                                                          radius: DiscordVoiceSupport.dotSide / 2))
                    .frame(width: wing, height: height, alignment: .trailing)
            }
            .frame(height: height)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, geometry.compactActivityHorizontalPadding)
        .padding(.top, geometry.compactActivityTopPadding)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(FeatureStrings.notchVoice(l10n.language).title)
        .accessibilityValue(FeatureStrings.notchVoice(l10n.language).status(reading))
        .accessibilityHint(FeatureStrings.notch(l10n.language).open)
        .accessibilityIdentifier("notch.voiceChat")
    }
}
