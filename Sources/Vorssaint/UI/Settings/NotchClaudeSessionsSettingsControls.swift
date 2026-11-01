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
    @AppStorage(DefaultsKey.notchSessionsBuddies) private var buddies = true
    @AppStorage(DefaultsKey.notchSessionsLimits) private var limits = true

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
            switchRow("gauge.with.dots.needle.50percent", text.limitsSetting, caption: text.limitsSettingHint, isOn: $limits)
            buddiesRow
            Divider()
            hooks
            codex
            bridge
        }
        .onAppear {
            service.checkHooks()
            service.checkCodex()
            service.checkBridge()
        }
    }

    /// A few Clawds in their costumes beside the words, so the switch shows
    /// what it is about even with no session running.
    private var buddiesRow: some View {
        VStack(alignment: .leading, spacing: 4) {
            SettingsRow(symbol: "tshirt", title: text.buddies) {
                Toggle(text.buddies, isOn: $buddies).labelsHidden().toggleStyle(.switch)
            }
            HStack(alignment: .center, spacing: 12) {
                Text(text.buddiesHint)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                HStack(spacing: 4) {
                    ForEach(Self.samples, id: \.costume) { sample in
                        NotchClawdPicture(costume: sample.costume, character: sample.character,
                                          size: CGSize(width: 30, height: 28))
                    }
                }
                .opacity(buddies ? 1 : 0.35)
                .accessibilityHidden(true)
            }
            .padding(.leading, settingsRowTextInset)
        }
    }

    private static let samples: [(costume: ClawdCostume, character: ClawdCharacter)] = [
        (ClawdCostume.classic, .clawd),
        (ClawdCostume(shell: .iris, hat: .cap, eyewear: .none, extra: .none), .codex),
        (ClawdCostume(shell: .moss, hat: .topHat, eyewear: .shades, extra: .none), .clawd),
    ]

    /// Codex needs nothing installed: its row only says whether the island hears it.
    private var codex: some View {
        VStack(alignment: .leading, spacing: 6) {
            SettingsRow(symbol: "terminal.fill", title: "Codex", caption: service.codexStatus.map {
                switch $0 {
                case .connected: return text.codexConnected
                case .notRunning: return text.codexNotRunning
                case .notFound: return text.codexNotFound
                }
            }) {
                if service.codexStatus == nil { ProgressView().controlSize(.small) }
            }
            Text(text.codexHint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, settingsRowTextInset)
        }
    }

    /// One row per editor built on VS Code on this Mac, each with its own
    /// install button, since each keeps its own extensions.
    private var bridge: some View {
        VStack(alignment: .leading, spacing: 6) {
            SettingsRow(symbol: "terminal", title: text.bridgeTitle,
                        caption: service.bridges?.isEmpty == true ? text.bridgeNoEditors : nil) {
                if service.bridges == nil { ProgressView().controlSize(.small) }
            }
            ForEach(service.bridges ?? []) { bridge in
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 8) {
                        Text(bridge.ide.name)
                            .font(.callout)
                        Text(bridge.installed ? text.hooksInstalled : text.hooksMissing)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Spacer(minLength: 8)
                        if !bridge.installed || bridge.outdated {
                            Button(bridge.installed ? text.updateBridge : text.installBridge) { service.installBridge(bridge.ide) }
                                .controlSize(.small)
                                .fixedSize()
                        }
                    }
                    if let failure = service.bridgeFailures[bridge.ide.bundle] {
                        Label(failure, systemImage: "exclamationmark.triangle.fill")
                            .font(.caption)
                            .foregroundStyle(.orange)
                    }
                }
                .padding(.leading, settingsRowTextInset)
            }
            Text(text.bridgeHint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, settingsRowTextInset)
        }
    }

    private var hooks: some View {
        VStack(alignment: .leading, spacing: 6) {
            // The state reads under the title, so the button keeps its whole name.
            SettingsRow(symbol: "link", title: text.hooksTitle,
                        caption: service.hooksInstalled.map { $0 ? text.hooksInstalled : text.hooksMissing }) {
                if let installed = service.hooksInstalled {
                    Button(installed ? text.removeHooks : text.installHooks) { service.installHooks(!installed) }
                        .controlSize(.small)
                        .fixedSize()
                } else {
                    ProgressView().controlSize(.small)
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
