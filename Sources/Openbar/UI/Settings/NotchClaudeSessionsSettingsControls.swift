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
    @AppStorage(DefaultsKey.notchSessionsProgressStyle) private var progressStyle = NotchSessionProgressStyle.ring.rawValue
    @AppStorage(DefaultsKey.notchSessionsGrouping) private var grouping = NotchSessionGrouping.none.rawValue
    @AppStorage(DefaultsKey.notchSessionsAwayCard) private var awayCard = true
    @AppStorage(DefaultsKey.notchSessionsAwayMinutes) private var awayMinutes = 10
    @AppStorage(DefaultsKey.notchSessionsRemindPermission) private var remindPermission = false
    @AppStorage(DefaultsKey.notchSessionsRemindQuestion) private var remindQuestion = false
    @AppStorage(DefaultsKey.notchSessionsRemindFinished) private var remindFinished = false
    @AppStorage(DefaultsKey.notchSessionsRemindAfter) private var remindAfter = 5
    @AppStorage(DefaultsKey.notchSessionsRemindEvery) private var remindEvery = 0
    @AppStorage(DefaultsKey.notchSessionsRemindDuringFocus) private var remindDuringFocus = false
    @AppStorage(DefaultsKey.notchSessionsVoiceReplies) private var voiceReplies = true
    @AppStorage(DefaultsKey.notchSessionsVoiceLanguage) private var voiceLanguage = ""
    @AppStorage(DefaultsKey.notchSessionsVoiceSendOnStop) private var voiceSendOnStop = false
    @AppStorage(DefaultsKey.notchSessionsVoiceShortcutEnabled) private var voiceShortcut = false
    @State private var voiceLanguages: [VoiceReplyLanguage] = []

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(text.settingsDescription)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            switchRow("waveform.path.ecg", text.liveActivity, isOn: $liveActivity)
            switchRow("rectangle.expand.vertical", text.autoExpand, caption: text.autoExpandHint, isOn: $autoExpand)
            SessionSoundsSettings()
            reminders
            SessionAutoModeSettings()
            switchRow("text.bubble", text.contentPreview, caption: text.contentPreviewHint, isOn: $previews)
            switchRow("gauge.with.dots.needle.50percent", text.limitsSetting, caption: text.limitsSettingHint, isOn: $limits)
            buddiesRow
            progress
            voice
            Divider()
            hooksLink
            ClaudeAccountsSettings()
            codex
            agents
            bridge
            RemoteHostsSettings()
        }
        .onAppear {
            service.checkHooks()
            service.checkCodex()
            service.checkAgentHooks()
            service.checkHookTargets()
            service.checkBridge()
        }
    }

    /// How a task list shows on a row, and the card on return (#215).
    private var progress: some View {
        VStack(alignment: .leading, spacing: 10) {
            // One list, or groups by project or agent (#233).
            SettingsRow(symbol: "square.stack.3d.up", title: text.groupingSetting, caption: text.groupingSettingHint) {
                Picker(text.groupingSetting, selection: $grouping) {
                    Text(text.groupNone).tag(NotchSessionGrouping.none.rawValue)
                    Text(text.groupByProject).tag(NotchSessionGrouping.project.rawValue)
                    Text(text.groupByAgent).tag(NotchSessionGrouping.agent.rawValue)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(maxWidth: 260)
            }
            SettingsRow(symbol: "circle.dashed", title: text.progressStyle, caption: text.progressStyleHint) {
                Picker(text.progressStyle, selection: $progressStyle) {
                    Text(text.progressRing).tag(NotchSessionProgressStyle.ring.rawValue)
                    Text(text.progressDots).tag(NotchSessionProgressStyle.dots.rawValue)
                    Text(text.progressBar).tag(NotchSessionProgressStyle.bar.rawValue)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(maxWidth: 200)
            }
            switchRow("moon.stars", text.awayCard, caption: text.awayCardHint, isOn: $awayCard)
            if awayCard {
                SettingsRow(symbol: "timer", title: text.awayMinutes) {
                    Picker(text.awayMinutes, selection: $awayMinutes) {
                        ForEach([5, 10, 15, 30, 60], id: \.self) { Text(text.minutes($0)).tag($0) }
                    }
                    .labelsHidden()
                    .frame(maxWidth: 120)
                }
            }
        }
    }

    /// Reminders of a session nobody reacted to (#231): what to remind of,
    /// then when, once any of it is on.
    private var reminders: some View {
        VStack(alignment: .leading, spacing: 10) {
            SettingsRow(symbol: "bell.badge", title: text.remindersSetting, caption: text.remindersSettingHint) {
                EmptyView()
            }
            VStack(alignment: .leading, spacing: 6) {
                Toggle(text.remindPermission, isOn: $remindPermission)
                Toggle(text.remindQuestion, isOn: $remindQuestion)
                Toggle(text.remindFinished, isOn: $remindFinished)
            }
            .toggleStyle(.checkbox)
            .padding(.leading, settingsRowTextInset)
            if remindPermission || remindQuestion || remindFinished {
                SettingsRow(symbol: "timer", title: text.remindAfter) {
                    Picker(text.remindAfter, selection: $remindAfter) {
                        ForEach(NotchSessionReminders.Settings.afterChoices, id: \.self) { Text(text.minutes($0)).tag($0) }
                    }
                    .labelsHidden()
                    .frame(maxWidth: 120)
                }
                SettingsRow(symbol: "arrow.clockwise", title: text.remindEvery) {
                    Picker(text.remindEvery, selection: $remindEvery) {
                        ForEach(NotchSessionReminders.Settings.everyChoices, id: \.self) { minutes in
                            Text(minutes == 0 ? text.remindOnce : text.remindEveryMinutes(minutes)).tag(minutes)
                        }
                    }
                    .labelsHidden()
                    .frame(maxWidth: 140)
                }
                switchRow("moon", text.remindDuringFocus, caption: text.remindDuringFocusHint, isOn: $remindDuringFocus)
            }
        }
    }

    /// Speaking a reply: the switch, then the language, the hold-to-talk
    /// shortcut and sending by itself, once it is on.
    private var voice: some View {
        let strings = NotchVoiceReplyStrings.forLanguage(l10n.language)
        let appLocale = VoiceReplySupport.localeIdentifier(saved: "", app: l10n.language)
        return VStack(alignment: .leading, spacing: 10) {
            switchRow("mic", strings.settingTitle, caption: strings.settingHint, isOn: $voiceReplies)
            if voiceReplies {
                SettingsRow(symbol: "globe", title: strings.language) {
                    Picker(strings.language, selection: $voiceLanguage) {
                        Text(strings.followApp(Locale.current.localizedString(forIdentifier: appLocale) ?? appLocale)).tag("")
                        Divider()
                        ForEach(voiceLanguages) { language in
                            Text(language.onDevice ? language.name : language.name + " (" + strings.notOnDeviceTag + ")")
                                .tag(language.id)
                        }
                    }
                    .labelsHidden()
                    // The longest of the many language names would push the title out.
                    .frame(maxWidth: 220)
                }
                switchRow("keyboard", strings.shortcut, caption: strings.shortcutHint, isOn: $voiceShortcut)
                if voiceShortcut {
                    ShortcutPreferenceRow(role: .sessionVoice) {
                        NotchService.shared.syncWithPreferences()
                    }
                    .padding(.leading, settingsRowTextInset)
                }
                switchRow("paperplane", strings.sendOnStop, caption: strings.sendOnStopHint, isOn: $voiceSendOnStop)
            }
        }
        .task(id: voiceReplies) {
            guard voiceReplies, voiceLanguages.isEmpty else { return }
            voiceLanguages = await Task.detached(priority: .utility) { VoiceReplyLanguage.all() }.value
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

    /// Gemini CLI, Copilot CLI, OpenCode and Cursor: each found on this Mac
    /// gets its hooks or plugin by itself, and its switch shows it in the
    /// list or takes its hooks out.
    private var agents: some View {
        VStack(alignment: .leading, spacing: 8) {
            SettingsRow(symbol: "square.stack.3d.up", title: text.agentsTitle) {
                EmptyView()
            }
            Text(text.agentsHint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, settingsRowTextInset)
            ForEach(ClaudeSessionProvider.hookAgents, id: \.self) { provider in
                NotchSessionsAgentSettingsRow(provider: provider, text: text)
                    .padding(.leading, settingsRowTextInset)
            }
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

    /// Where each agent's hooks stand, and the page that checks and fixes
    /// them (#236).
    private var hooksLink: some View {
        let summary = service.hookTargets.map(HookTargets.summary)
        return SettingsRow(symbol: "point.3.connected.trianglepath.dotted", title: HooksPageStrings.linkTitle,
                           caption: summary.map { HooksPageStrings.linkCaption(working: $0.working, attention: $0.attention) }) {
            Button(HooksPageStrings.open) { SettingsRouter.shared.request(FeatureSettingsDestination(.hooks)) }
                .controlSize(.small)
                .fixedSize()
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

/// One agent's row: its mark and name, and the switch that shows its
/// sessions and keeps its hooks in. Where its hooks stand is on the Hooks page.
struct NotchSessionsAgentSettingsRow: View {
    let provider: ClaudeSessionProvider
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @AppStorage private var shown: Bool

    init(provider: ClaudeSessionProvider, text: NotchClaudeSessionStrings) {
        self.provider = provider
        self.text = text
        _shown = AppStorage(wrappedValue: true, NotchClaudeSessionsSupport.agentKey(provider) ?? "")
    }

    var body: some View {
        HStack(spacing: 8) {
            NotchHookAgentMark(provider: provider, size: 14, working: false)
                .frame(width: 18, height: 18)
            Text(provider.displayName)
                .font(.callout)
                .fixedSize()
            Spacer(minLength: 8)
            Toggle(provider.displayName, isOn: $shown)
                .labelsHidden()
                .toggleStyle(.switch)
                .controlSize(.small)
                .onChange(of: shown) { _, _ in service.syncWithPreferences() }
        }
        .opacity(shown ? 1 : 0.6)
    }
}
