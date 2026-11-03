// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The microphone beside a reply field. Pressed, it listens and grows into
/// a level meter with a stop button; pressed again, it stops.
struct NotchVoiceReplyButton: View {
    let session: ClaudeSession
    let side: CGFloat
    @ObservedObject private var voice = VoiceReplyRecorder.shared
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var strings: NotchVoiceReplyStrings { .forLanguage(l10n.language) }
    private var active: Bool { voice.isRecording(session) }
    private var listening: Bool { active && voice.machine.isRecording }

    var body: some View {
        Button { voice.toggle(session) } label: {
            HStack(spacing: side * 0.16) {
                if listening {
                    NotchVoiceLevelMeter(level: voice.level, height: side * 0.46, reduceMotion: reduceMotion)
                        .transition(.opacity)
                }
                if active, !listening {
                    // Asking for permission, or the last words on their way.
                    ProgressView().controlSize(.mini).tint(.black.opacity(0.7))
                } else {
                    Image(systemName: listening ? "stop.fill" : "mic.fill")
                        .font(.system(size: side * (listening ? 0.34 : 0.44), weight: .semibold))
                }
            }
            .foregroundStyle(active ? Color.black.opacity(0.85) : .white.opacity(0.6))
            .padding(.horizontal, listening ? side * 0.32 : 0)
            .frame(minWidth: side, minHeight: side, maxHeight: side)
            .background(active ? session.provider.tint : .white.opacity(0.08), in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: side / 2))
        .help(listening ? strings.stop + " · " + strings.escCancels : strings.start)
        .accessibilityLabel(listening ? strings.stop : strings.start)
        .animation(reduceMotion ? nil : .spring(duration: 0.28, bounce: 0.2), value: listening)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: active)
    }
}

/// Five bars that follow the microphone. With Reduce Motion they hold still
/// and light up instead of growing.
struct NotchVoiceLevelMeter: View {
    let level: Float
    let height: CGFloat
    let reduceMotion: Bool
    private static let weights: [CGFloat] = [0.45, 0.75, 1, 0.75, 0.45]

    var body: some View {
        HStack(spacing: max(1.5, height * 0.14)) {
            ForEach(Self.weights.indices, id: \.self) { index in
                Capsule()
                    .frame(width: max(2, height * 0.18), height: barHeight(index))
                    .opacity(reduceMotion ? (CGFloat(level) * 5 >= CGFloat(index) + 0.5 ? 1 : 0.3) : 1)
            }
        }
        .frame(height: height)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.1), value: level)
        .accessibilityHidden(true)
    }

    private func barHeight(_ index: Int) -> CGFloat {
        if reduceMotion { return height * 0.7 }
        let lifted = max(0.18, min(1, CGFloat(level) * 1.4))
        return max(2, height * Self.weights[index] * lifted)
    }
}

/// Why a voice reply could not happen, under the field, with the settings
/// page that fixes it.
struct NotchVoiceReplyProblemLabel: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    let fontSize: CGFloat
    @ObservedObject private var voice = VoiceReplyRecorder.shared
    @ObservedObject private var l10n = L10n.shared

    private var strings: NotchVoiceReplyStrings { .forLanguage(l10n.language) }

    var body: some View {
        if let problem = voice.problem(for: session) {
            HStack(spacing: 6) {
                Label(message(problem), systemImage: "mic.slash.fill")
                    .foregroundStyle(.orange)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                if let action = action(problem) {
                    Button(strings.openSettings) {
                        voice.clearProblem()
                        action()
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.white.opacity(0.85))
                    .underline()
                    .fixedSize()
                }
            }
            .font(.system(size: fontSize))
            .transition(.opacity)
        }
    }

    private func message(_ problem: VoiceReplyProblem) -> String {
        switch problem {
        case .noMicrophone: return strings.noMicrophone
        case .microphoneDenied: return strings.microphoneDenied
        case .speechDenied: return strings.speechDenied
        case .notOnDevice(let language): return strings.notOnDevice(language)
        case .recognizerFailed: return strings.recognizerFailed
        case .sendFailed(let failure): return NotchClaudeSessionFormat.problem(failure, text: text)
        }
    }

    private func action(_ problem: VoiceReplyProblem) -> (() -> Void)? {
        switch problem {
        case .noMicrophone: return { Permissions.shared.openSoundInputSettings() }
        case .microphoneDenied: return { Permissions.shared.openMicrophoneSettings() }
        case .speechDenied: return { Permissions.shared.openSpeechRecognitionSettings() }
        case .notOnDevice: return { Permissions.shared.openDictationSettings() }
        case .recognizerFailed, .sendFailed: return nil
        }
    }
}
