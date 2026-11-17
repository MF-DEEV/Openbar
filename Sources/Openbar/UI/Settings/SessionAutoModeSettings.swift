// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// Sessions in their agent's no-ask mode, per agent, and the opt-in to let
/// every request of an agent through (#234). Turning Always proceed on asks
/// first and says what it means.
struct SessionAutoModeSettings: View {
    @ObservedObject private var l10n = L10n.shared
    @AppStorage(DefaultsKey.notchSessionsAutoRunClaude) private var autoClaude = true
    @AppStorage(DefaultsKey.notchSessionsAutoRunCodex) private var autoCodex = true
    @AppStorage(DefaultsKey.notchSessionsAlwaysProceedClaude) private var alwaysClaude = false
    @AppStorage(DefaultsKey.notchSessionsAlwaysProceedCodex) private var alwaysCodex = false
    @AppStorage(DefaultsKey.notchSessionsAlwaysProceedOpenCode) private var alwaysOpenCode = false
    @AppStorage(DefaultsKey.notchSessionsAlwaysProceedVibe) private var alwaysVibe = false
    /// The agent whose Always proceed waits on the person's yes.
    @State private var confirming: ClaudeSessionProvider?

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            SettingsRow(symbol: "forward", title: text.autoModeSetting, caption: text.autoModeSettingHint) {
                EmptyView()
            }
            VStack(alignment: .leading, spacing: 6) {
                ForEach(SessionAutoMode.modeAgents, id: \.self) { provider in
                    Toggle(provider.displayName, isOn: autoRun(provider))
                }
            }
            .toggleStyle(.checkbox)
            .padding(.leading, settingsRowTextInset)
            SettingsRow(symbol: "exclamationmark.shield", title: text.alwaysProceedSetting,
                        caption: text.alwaysProceedSettingHint) {
                EmptyView()
            }
            VStack(alignment: .leading, spacing: 6) {
                ForEach(SessionAutoMode.proceedAgents, id: \.self) { provider in
                    VStack(alignment: .leading, spacing: 2) {
                        Toggle(provider.displayName, isOn: alwaysProceed(provider))
                        if always(provider).wrappedValue {
                            Label(text.alwaysProceedOnWarning, systemImage: "exclamationmark.triangle.fill")
                                .font(.caption)
                                .foregroundStyle(.orange)
                                .padding(.leading, 20)
                        }
                    }
                }
            }
            .toggleStyle(.checkbox)
            .padding(.leading, settingsRowTextInset)
        }
        .alert(text.alwaysProceedConfirmTitle(confirming?.displayName ?? ""), isPresented: Binding(
            get: { confirming != nil }, set: { if !$0 { confirming = nil } }
        )) {
            Button(text.alwaysProceedConfirm, role: .destructive) {
                if let provider = confirming { always(provider).wrappedValue = true }
                confirming = nil
            }
            Button(text.cancel, role: .cancel) { confirming = nil }
        } message: {
            Text(text.alwaysProceedConfirmMessage(confirming?.displayName ?? ""))
        }
    }

    private func autoRun(_ provider: ClaudeSessionProvider) -> Binding<Bool> {
        provider == .codex ? $autoCodex : $autoClaude
    }

    private func always(_ provider: ClaudeSessionProvider) -> Binding<Bool> {
        switch provider {
        case .codex: return $alwaysCodex
        case .opencode: return $alwaysOpenCode
        case .vibe: return $alwaysVibe
        default: return $alwaysClaude
        }
    }

    /// Off at once; on only after the person confirms.
    private func alwaysProceed(_ provider: ClaudeSessionProvider) -> Binding<Bool> {
        Binding(get: { always(provider).wrappedValue }, set: { on in
            if on { confirming = provider } else { always(provider).wrappedValue = false }
        })
    }
}
