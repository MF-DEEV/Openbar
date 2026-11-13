// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// First-run experience, also reachable later through Settings › About.
/// The person chooses what they want first; only then does the app explain and
/// request the permissions that choice actually needs.
enum OnboardingMode {
    case full

    func title(_ strings: Strings) -> String {
        strings.obStepWelcomeTitle
    }
}

enum OnboardingStep {
    case welcome, purpose, agents, sessionPermissions, permissions, tryIt, done
}

struct OnboardingView: View {
    var mode: OnboardingMode = .full
    var onFinish: () -> Void

    @ObservedObject private var l10n = L10n.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Persisted so the flow resumes where it stopped — macOS relaunches the
    /// app when Screen Recording is granted mid-onboarding.
    @AppStorage(DefaultsKey.onboardingStep) private var index = 0
    @State private var selectedFeatures: Set<AppFeature>
    @State private var selectedPreset: FeaturePreset?
    @State private var forward = true

    init(mode: OnboardingMode = .full, onFinish: @escaping () -> Void) {
        self.mode = mode
        self.onFinish = onFinish
        let defaults = UserDefaults.standard
        let selectionWasApplied = defaults.bool(forKey: DefaultsKey.hasOnboarded)
            || defaults.integer(forKey: DefaultsKey.onboardingStep) >= 2
        let features = selectionWasApplied
            ? Set(AppFeature.allCases.filter(\.isAvailable))
            : FeaturePreset.recommended.features
        _selectedFeatures = State(initialValue: features)
        _selectedPreset = State(initialValue: selectionWasApplied ? nil : .recommended)
    }

    private var setupPermissions: [AppPermission] {
        let permissions = Set(selectedFeatures.flatMap(\.onboardingPermissions))
        return [.accessibility, .screenRecording].filter { permissions.contains($0) }
    }
    /// Coding agents chosen: the flow is about sessions (#168). The toolkit's
    /// own permissions follow only when something chosen asks for them.
    private var agentsChosen: Bool { selectedFeatures.contains(.notchAgents) && selectedFeatures.contains(.notch) }
    private var steps: [OnboardingStep] {
        guard agentsChosen else { return [.welcome, .purpose, .permissions, .done] }
        return [.welcome, .purpose, .agents, .sessionPermissions]
            + (setupPermissions.isEmpty ? [] : [.permissions]) + [.tryIt, .done]
    }
    private var current: OnboardingStep { steps[min(max(0, index), steps.count - 1)] }

    var body: some View {
        VStack(spacing: 0) {
            // Scrollable so a step taller than the window (the menu bar
            // metrics list outgrew it, issue #176) can never push the
            // navigation bar out of the fixed frame — without a scroll, the
            // footer was clipped away and the flow looked stuck.
            ScrollView {
                content
                    .id(current)
                    .transition(reduceMotion ? .opacity : .asymmetric(
                        insertion: .move(edge: forward ? .trailing : .leading).combined(with: .opacity),
                        removal: .opacity))
                    .frame(maxWidth: .infinity, alignment: .top)
            }
            .scrollBounceBehavior(.basedOnSize)

            Divider()
            navigationBar
        }
        .frame(width: 560, height: 640)
        .onAppear {
            if !steps.indices.contains(index) { index = 0 }
            // Onboarding narrates the permission trip itself; the floating
            // guide card would just double the voice.
            PermissionGuideOverlay.suppressed = true
        }
        .onDisappear {
            PermissionGuideOverlay.suppressed = false
            ClaudeSessionService.shared.dismissSample()
        }
    }

    @ViewBuilder
    private var content: some View {
        switch current {
        case .welcome: WelcomeStep()
        case .purpose: PurposeStep(selectedFeatures: $selectedFeatures,
                                   selectedPreset: $selectedPreset)
        case .agents: OnboardingAgentsStep()
        case .sessionPermissions: OnboardingSessionPermissionsStep()
        case .permissions: SelectedPermissionsStep(features: selectedFeatures,
                                                   permissions: setupPermissions)
        case .tryIt: OnboardingTryStep()
        case .done: DoneStep(agents: agentsChosen)
        }
    }

    private func go(_ step: Int) {
        forward = step > index
        withAnimation(reduceMotion ? .easeInOut(duration: 0.15) : .spring(response: 0.45, dampingFraction: 0.86)) {
            index = step
        }
    }

    private var navigationBar: some View {
        HStack {
            Button(l10n.s.obBack) { go(max(0, index - 1)) }
                .disabled(index == 0)

            Spacer()

            HStack(spacing: 6) {
                ForEach(steps.indices, id: \.self) { i in
                    Capsule()
                        .fill(i == index ? Color.brandAccent : Color.primary.opacity(0.15))
                        .frame(width: i == index ? 18 : 7, height: 7)
                        .animation(.spring(response: 0.3, dampingFraction: 0.8), value: index)
                }
            }
            .accessibilityHidden(true)

            Spacer()

            Button(primaryButtonTitle) {
                if index >= steps.count - 1 {
                    index = 0
                    onFinish()
                } else {
                    if current == .purpose {
                        FeatureRuntime.shared.replaceAvailable(
                            with: selectedFeatures,
                            enabling: selectedPreset?.enableKeys ?? Self.agentKeys(selectedFeatures))
                    }
                    go(index + 1)
                }
            }
            .keyboardShortcut(.defaultAction)
            .controlSize(.large)
            .buttonStyle(.borderedProminent)
        }
        .padding(16)
    }

    /// Agents picked one by one switch the island and its sessions on too,
    /// as the preset would.
    private static func agentKeys(_ features: Set<AppFeature>) -> [String] {
        features.contains(.notchAgents) && features.contains(.notch) ? FeaturePreset.agents.enableKeys : []
    }

    private var primaryButtonTitle: String {
        if index >= steps.count - 1 { return l10n.s.obStart }
        return l10n.s.obContinue
    }
}

// MARK: - Step 1: welcome & language

private struct WelcomeStep: View {
    @ObservedObject private var l10n = L10n.shared
    private var text: OnboardingStrings { FeatureStrings.onboarding(l10n.language) }

    var body: some View {
        VStack(spacing: 0) {
            OnboardingIslandDemo(height: 290)

            VStack(spacing: 10) {
                Text(text.welcomeTitle)
                    .font(.system(size: 26, weight: .bold))
                    .multilineTextAlignment(.center)
                Text(text.welcomeBody)
                    .font(.system(size: 13.5))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 30)
                Text(text.welcomeToolkit)
                    .font(.system(size: 12))
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.center)
            }
            .padding(.top, 26)
            .padding(.horizontal, 30)

            Picker(selection: $l10n.language) {
                ForEach(AppLanguage.allCases) { language in
                    Text(language.displayName).tag(language)
                }
            } label: {
                Image(systemName: "globe")
                    .accessibilityLabel(l10n.s.obLanguageLabel)
            }
            // A menu (not segmented): with fifteen languages a segmented control
            // would overflow, and several names are in their own script.
            .pickerStyle(.menu)
            .fixedSize()
            .padding(.top, 22)
            .padding(.bottom, 16)
        }
    }
}

// MARK: - Purpose step

/// Presets are quick starts, while the catalog below allows an exact choice.
/// Both use the Features hub's own names and descriptions so setup stays
/// consistent with what the person can change later.
private struct PurposeStep: View {
    @ObservedObject private var l10n = L10n.shared
    @Binding var selectedFeatures: Set<AppFeature>
    @Binding var selectedPreset: FeaturePreset?

    private var hub: FeatureHubStrings { FeatureStrings.hub(l10n.language) }

    @State private var showsCatalog = false
    private var text: OnboardingStrings { FeatureStrings.onboarding(l10n.language) }
    private var toolkit: [FeaturePreset] { FeaturePreset.allCases.filter { $0 != .agents } }

    var body: some View {
        VStack(spacing: 16) {
            Text(text.setupTitle)
                .font(.system(size: 22, weight: .bold))
                .padding(.top, 30)

            agentsCard
                .padding(.horizontal, 28)

            VStack(alignment: .leading, spacing: 8) {
                Text(text.setupToolkit)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    ForEach(toolkit) { toolkitCard($0) }
                }
            }
            .padding(.horizontal, 28)

            DisclosureGroup(isExpanded: $showsCatalog) {
                catalog.padding(.top, 10)
            } label: {
                HStack {
                    Text(text.setupOneByOne)
                        .font(.system(size: 12.5, weight: .medium))
                    Spacer()
                    Text(String(format: hub.activeCountFormat,
                                selectedFeatures.count, FeatureRuntime.shared.installableCount))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
                .contentShape(Rectangle())
                .onTapGesture { withAnimation(.easeInOut(duration: 0.2)) { showsCatalog.toggle() } }
            }
            .padding(.horizontal, 28)
            .padding(.bottom, 16)
        }
    }

    private var catalog: some View {
        LazyVStack(spacing: 14) {
            ForEach(FeatureGroup.allCases, id: \.self) { group in
                VStack(alignment: .leading, spacing: 7) {
                    Text(groupTitle(group))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ForEach(AppFeature.features(in: group), id: \.self) { feature in
                        featureCard(feature)
                    }
                }
            }
        }
    }

    private func pick(_ preset: FeaturePreset) {
        withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) {
            selectedPreset = preset
            selectedFeatures = preset.features
        }
    }

    /// The setup most people came for, first and largest, with the agents it
    /// covers drawn in a row.
    private var agentsCard: some View {
        let selected = selectedPreset == .agents
        return Button { pick(.agents) } label: {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 8) {
                    ForEach(ClaudeSessionProvider.allCases, id: \.self) { provider in
                        Circle()
                            .fill(Color.black)
                            .frame(width: 34, height: 34)
                            .overlay(NotchSessionAgentGlyph(provider: provider, size: 13, working: selected))
                    }
                    Spacer()
                    Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                        .font(.system(size: 20, weight: .semibold))
                        .foregroundStyle(selected ? Color.brandAccent : Color.secondary.opacity(0.4))
                        .contentTransition(.symbolEffect(.replace))
                }
                VStack(alignment: .leading, spacing: 3) {
                    Text(text.setupAgentsName)
                        .font(.system(size: 16, weight: .bold))
                        .foregroundStyle(.primary)
                    Text(text.setupAgentsDesc)
                        .font(.system(size: 12.5))
                        .foregroundStyle(.secondary)
                }
            }
            .padding(16)
            .background(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(selected ? Color.brandAccent.opacity(0.12) : Color.primary.opacity(0.05))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .strokeBorder(selected ? Color.brandAccent.opacity(0.55) : Color.primary.opacity(0.06),
                                  lineWidth: selected ? 1.5 : 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(OnboardingPressStyle())
        .accessibilityLabel("\(text.setupAgentsName). \(text.setupAgentsDesc)")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func toolkitCard(_ preset: FeaturePreset) -> some View {
        let selected = selectedPreset == preset
        return Button { pick(preset) } label: {
            VStack(alignment: .leading, spacing: 8) {
                Image(systemName: preset.symbolName)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(selected ? Color.brandAccent : Color.secondary)
                    .frame(height: 18)
                    .accessibilityHidden(true)
                Text(name(preset))
                    .font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(.primary)
                Text(caption(preset))
                    .font(.system(size: 10.5))
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, minHeight: 104, alignment: .topLeading)
            .padding(12)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(selected ? Color.brandAccent.opacity(0.12) : Color.primary.opacity(0.05))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(selected ? Color.brandAccent.opacity(0.5) : .clear, lineWidth: 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
        .buttonStyle(OnboardingPressStyle())
        .accessibilityLabel("\(name(preset)). \(caption(preset))")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func name(_ preset: FeaturePreset) -> String {
        switch preset {
        case .agents: return text.setupAgentsName
        case .essential: return hub.presetEssentialName
        case .windows: return hub.presetWindowsName
        case .battery: return hub.presetBatteryName
        }
    }

    private func caption(_ preset: FeaturePreset) -> String {
        switch preset {
        case .agents: return text.setupAgentsDesc
        case .essential: return hub.presetEssentialDesc
        case .windows: return hub.presetWindowsDesc
        case .battery: return hub.presetBatteryDesc
        }
    }

    private func featureCard(_ feature: AppFeature) -> some View {
        let selected = selectedFeatures.contains(feature)
        // The picker writes availability through the same runtime gate as the
        // hub, so a feature this Mac cannot run would silently stay off after
        // being ticked here. It is shown and refused instead, exactly as the
        // hub row does it.
        let blocked = feature.installBlockedReason
        let card = Button {
            selectedPreset = nil
            if selected {
                selectedFeatures.remove(feature)
                if feature == .notch {
                    selectedFeatures.subtract(AppFeature.dynamicIslandExtensions)
                }
            } else {
                selectedFeatures.formUnion(feature.initialInstallGroup)
            }
        } label: {
            HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(selected
                          ? AnyShapeStyle(Theme.spaceGradient)
                          : AnyShapeStyle(Color.secondary.opacity(0.16)))
                    .frame(width: 32, height: 32)
                    .overlay {
                        Image(systemName: feature.symbolName)
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(selected ? AnyShapeStyle(.white) : AnyShapeStyle(.secondary))
                            .accessibilityHidden(true)
                    }
                VStack(alignment: .leading, spacing: 2) {
                    Text(feature.hubTitle(l10n.s, hub: hub))
                        .font(.system(size: 12.5, weight: .semibold))
                        .foregroundStyle(.primary)
                    Text(feature.hubDescription(hub))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 8)
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(selected ? Color.brandAccent : Color.secondary.opacity(0.45))
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, 11)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(selected ? Color.brandAccent.opacity(0.09) : Color.primary.opacity(0.035))
            )
            .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(blocked != nil)
        .opacity(blocked == nil ? 1 : 0.4)
        .saturation(blocked == nil ? 1 : 0)
        .accessibilityLabel("\(feature.hubTitle(l10n.s, hub: hub)). \(feature.hubDescription(hub))"
                            + (blocked.map { ". \($0)" } ?? ""))
        .accessibilityAddTraits(selected ? .isSelected : [])
        // .help() never fires on a disabled control, so the tooltip sits on a
        // wrapper outside it, the same way the Features hub row does it.
        return HStack(spacing: 0) { card }.help(blocked ?? "")
    }

    private func groupTitle(_ group: FeatureGroup) -> String {
        switch group {
        case .windowsDock: return hub.groupWindowsDock
        case .mouseKeyboard: return hub.groupMouseKeyboard
        case .clipboardFiles: return hub.groupClipboardFiles
        case .sound: return hub.groupSound
        case .energyDisplay: return hub.groupEnergyDisplay
        case .tools: return hub.groupTools
        case .dynamicIsland: return FeatureStrings.notch(l10n.language).title
        case .monitor: return hub.groupMonitor
        }
    }
}

private struct SelectedPermissionsStep: View {
    @ObservedObject private var l10n = L10n.shared
    @State private var showingOtherPermissions = false
    let features: Set<AppFeature>
    let permissions: [AppPermission]

    private var hub: FeatureHubStrings { FeatureStrings.hub(l10n.language) }
    private var otherPermissions: [AppPermission] {
        AppPermission.allCases.filter { !permissions.contains($0) }
    }

    var body: some View {
        VStack(spacing: 18) {
            StepHeader(icon: "key.horizontal.fill",
                       title: hub.tabPermissions,
                       subtitle: hub.permissionsIntro)

            VStack(alignment: .leading, spacing: 12) {
                Text(hub.onboardingSelectedPermissionsTitle)
                    .font(.headline)

                if permissions.isEmpty {
                    Label(hub.onboardingNoSelectedPermissions,
                          systemImage: "checkmark.circle.fill")
                        .foregroundStyle(.secondary)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(permissions.enumerated()), id: \.offset) { index, permission in
                            if index > 0 { Divider().padding(.vertical, 12) }
                            VStack(alignment: .leading, spacing: 8) {
                                PermissionRow(kind: permission == .accessibility
                                              ? .accessibility : .screenRecording)
                                Text(permission.explainer(hub))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text(String(format: hub.usedByFormat,
                                            featureNames(for: permission)))
                                    .font(.caption)
                                    .foregroundStyle(.tertiary)
                            }
                        }
                    }
                }
            }
            .padding(16)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(Color.primary.opacity(0.05))
            )
            .padding(.horizontal, 28)

            DisclosureGroup(isExpanded: $showingOtherPermissions) {
                VStack(alignment: .leading, spacing: 14) {
                    PermissionsPortalSections(hub: hub,
                                              visiblePermissions: otherPermissions)
                }
                .padding(.top, 12)
            } label: {
                VStack(alignment: .leading, spacing: 3) {
                    Text(hub.onboardingOtherPermissionsTitle)
                        .font(.headline)
                    Text(hub.onboardingOtherPermissionsCaption)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(16)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(Color.primary.opacity(0.05))
            )
            .padding(.horizontal, 28)

            Text(l10n.s.permissionRestartNote)
                .font(.caption)
                .foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 36)

            Spacer()
        }
    }

    private func featureNames(for permission: AppPermission) -> String {
        features
            .filter { $0.onboardingPermissions.contains(permission) }
            .map { $0.hubTitle(l10n.s, hub: hub) }
            // Localized: a plain sort orders by Unicode scalar, which throws
            // every accented name past Z. On the first screen someone sees,
            // in a language with accents, that reads as a list in no order.
            .sorted { $0.localizedStandardCompare($1) == .orderedAscending }
            .joined(separator: ", ")
    }
}

// MARK: - Done

private struct DoneStep: View {
    var agents: Bool
    @ObservedObject private var l10n = L10n.shared
    private var text: OnboardingStrings { FeatureStrings.onboarding(l10n.language) }

    var body: some View {
        VStack(spacing: 0) {
            OnboardingMenuBarScene(showsNotch: agents)
                .frame(height: 200)

            VStack(spacing: 10) {
                Text(text.doneTitle)
                    .font(.system(size: 26, weight: .bold))
                if agents {
                    Text(text.doneBody)
                        .font(.system(size: 13.5))
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 50)
                }
            }
            .padding(.top, 30)

            if agents {
                OnboardingSessionKeys()
                    .padding(.top, 30)
            }
            Spacer(minLength: 0)
        }
    }
}

/// Cards that sink a little under the pointer's press.
struct OnboardingPressStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.97 : 1)
            .animation(.spring(response: 0.25, dampingFraction: 0.7), value: configuration.isPressed)
    }
}

// MARK: - Shared header

private struct StepHeader: View {
    let icon: String
    let title: String
    let subtitle: String

    var body: some View {
        VStack(spacing: 10) {
            ZStack {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(Theme.spaceGradient)
                    .frame(width: 56, height: 56)
                Image(systemName: icon)
                    .font(.system(size: 24, weight: .semibold))
                    .foregroundStyle(.white)
            }
            Text(title)
                .font(.system(size: 19, weight: .bold))
            Text(subtitle)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 48)
        }
        .padding(.top, 30)
    }
}
