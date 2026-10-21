// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The Sessions page's options, under its row in the Dynamic Island settings.
struct NotchClaudeSessionsSettingsControls: View {
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var service = ClaudeSessionService.shared
    @AppStorage(DefaultsKey.notchSessionsLiveActivity) private var liveActivity = true
    @AppStorage(DefaultsKey.notchSessionsAutoExpand) private var autoExpand = true
    @AppStorage(DefaultsKey.notchSessionsContentPreview) private var previews = false

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(text.settingsDescription)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            switchRow("waveform.path.ecg", text.liveActivity, isOn: $liveActivity)
            switchRow("rectangle.expand.vertical", text.autoExpand, caption: text.autoExpandHint, isOn: $autoExpand)
            switchRow("text.bubble", text.contentPreview, caption: text.contentPreviewHint, isOn: $previews)
            Divider()
            hooks
            bridge
        }
        .onAppear {
            service.checkHooks()
            service.checkBridge()
        }
    }

    private var bridge: some View {
        VStack(alignment: .leading, spacing: 6) {
            SettingsRow(symbol: "terminal", title: text.bridgeTitle) {
                HStack(spacing: 8) {
                    if let installed = service.bridgeInstalled {
                        Label(installed ? text.hooksInstalled : text.hooksMissing,
                              systemImage: installed ? "checkmark.circle.fill" : "circle.dashed")
                            .font(.caption)
                            .foregroundStyle(installed ? .green : .secondary)
                        if !installed { Button(text.installBridge) { service.installBridge() }.controlSize(.small) }
                    } else {
                        ProgressView().controlSize(.small)
                    }
                }
            }
            Text(text.bridgeHint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, settingsRowTextInset)
            if let failure = service.bridgeFailure {
                Label(failure, systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .padding(.leading, settingsRowTextInset)
            }
        }
    }

    private var hooks: some View {
        VStack(alignment: .leading, spacing: 6) {
            SettingsRow(symbol: "link", title: text.hooksTitle) {
                HStack(spacing: 8) {
                    if let installed = service.hooksInstalled {
                        Label(installed ? text.hooksInstalled : text.hooksMissing,
                              systemImage: installed ? "checkmark.circle.fill" : "circle.dashed")
                            .font(.caption)
                            .foregroundStyle(installed ? .green : .secondary)
                        Button(installed ? text.removeHooks : text.installHooks) { service.installHooks(!installed) }
                            .controlSize(.small)
                    } else {
                        ProgressView().controlSize(.small)
                    }
                }
            }
            Text(text.hooksHint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, settingsRowTextInset)
            if let failure = service.hookFailure {
                Label(text.hooksFailed(failure), systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .padding(.leading, settingsRowTextInset)
            }
        }
    }

    private func switchRow(_ symbol: String, _ title: String, caption: String? = nil, isOn: Binding<Bool>) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            SettingsRow(symbol: symbol, title: title) {
                Toggle(title, isOn: isOn).labelsHidden().toggleStyle(.switch)
            }
            if let caption {
                Text(caption)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.leading, settingsRowTextInset)
            }
        }
    }
}
