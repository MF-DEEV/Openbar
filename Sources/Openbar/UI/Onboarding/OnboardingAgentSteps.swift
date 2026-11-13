// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AVFoundation
import AppKit
import Speech
import SwiftUI

// The steps onboarding shows for coding agents (#168). Each one shows its
// point instead of describing it: a request playing out in a drawn island,
// the agents found on this Mac connecting one by one, a real sample request
// in the notch, and where Openbar lives afterwards.

// MARK: - The island, drawn

/// A request playing out in a drawn notch, on a loop: the island opens on a
/// session that needs permission, Allow is pressed, the command runs and the
/// island closes again, each round with another agent. It uses the island's
/// own words, so what it shows is what the real one says. With Reduce Motion
/// on it holds still on the request.
struct OnboardingIslandDemo: View {
    var height: CGFloat = 250
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @ObservedObject private var l10n = L10n.shared
    @State private var phase = Phase.resting
    @State private var round = 0

    private enum Phase: Int { case resting, asking, pressing, allowed, closing }

    private static let rounds: [(provider: ClaudeSessionProvider, project: String, command: String)] = [
        (.claude, "checkout-api", "npm test"),
        (.codex, "mobile-app", "pod install"),
        (.gemini, "landing-page", "git push"),
        (.opencode, "infra", "terraform plan"),
        (.copilot, "docs-site", "bun run build"),
        (.cursor, "design-system", "rm -rf dist"),
    ]

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }
    private var current: (provider: ClaudeSessionProvider, project: String, command: String) {
        Self.rounds[round % Self.rounds.count]
    }
    private var open: Bool { phase != .resting && phase != .closing }

    var body: some View {
        ZStack(alignment: .top) {
            backdrop
            island
                .padding(.top, 0)
        }
        .frame(height: height)
        .frame(maxWidth: .infinity)
        .clipped()
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(current.provider.displayName): \(text.waitingPermission), \(current.command)")
        .task(id: reduceMotion) { await play() }
    }

    /// A dark desk with a soft light behind the notch, as the island sits on
    /// a real screen.
    private var backdrop: some View {
        ZStack {
            Theme.spaceGradient
            RadialGradient(colors: [Color.white.opacity(0.10), .clear],
                           center: .top, startRadius: 4, endRadius: height * 0.95)
            // The terminal the person no longer has to switch to, behind.
            terminal
                .offset(y: height * 0.30)
        }
    }

    private var terminal: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 5) {
                ForEach(0..<3, id: \.self) { _ in Circle().fill(Color.white.opacity(0.14)).frame(width: 7) }
            }
            .padding(.bottom, 4)
            Group {
                Text("$ \(current.provider.shortName.lowercased())").foregroundStyle(.white.opacity(0.5))
                Text("● Reading \(current.project)/src …")
                Text("● Editing 3 files").opacity(phase == .resting ? 0 : 1)
                Text("● \(current.command)").opacity(phase == .allowed || phase == .closing ? 1 : 0)
            }
            .font(.system(size: 10.5, design: .monospaced))
            .foregroundStyle(.white.opacity(0.32))
            Spacer(minLength: 0)
        }
        .padding(12)
        .frame(width: 330, height: 132, alignment: .topLeading)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.white.opacity(0.045)))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.white.opacity(0.07)))
        .animation(.easeOut(duration: 0.3), value: phase)
        .accessibilityHidden(true)
    }

    private var island: some View {
        let width: CGFloat = open ? 372 : 150
        let tall: CGFloat = open ? 116 : 30
        return ZStack(alignment: .top) {
            UnevenRoundedRectangle(bottomLeadingRadius: open ? 26 : 12, bottomTrailingRadius: open ? 26 : 12,
                                   style: .continuous)
                .fill(Color.black)
                .shadow(color: .black.opacity(open ? 0.55 : 0), radius: 18, y: 8)
            if open {
                card
                    .transition(.opacity.combined(with: .scale(scale: 0.92, anchor: .top)))
            } else {
                HStack {
                    NotchSessionAgentGlyph(provider: current.provider, size: 11, working: false)
                    Spacer()
                    if phase == .closing {
                        Image(systemName: "checkmark")
                            .font(.system(size: 10, weight: .bold))
                            .foregroundStyle(.green)
                            .transition(.scale.combined(with: .opacity))
                    }
                }
                .padding(.horizontal, 12)
                .frame(height: 30)
            }
        }
        .frame(width: width, height: tall)
        .animation(reduceMotion ? nil : .spring(response: 0.48, dampingFraction: 0.78), value: open)
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                NotchSessionAgentGlyph(provider: current.provider, size: 15, working: false)
                VStack(alignment: .leading, spacing: 1) {
                    Text(current.project)
                        .font(.system(size: 12.5, weight: .semibold))
                        .foregroundStyle(.white)
                    Text(phase == .allowed ? current.provider.displayName : text.waitingPermission)
                        .font(.system(size: 10.5, weight: .medium))
                        .foregroundStyle(phase == .allowed ? Color.white.opacity(0.55) : Color.orange)
                        .contentTransition(.opacity)
                }
                Spacer(minLength: 8)
                Text(current.command)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.white.opacity(0.85))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(RoundedRectangle(cornerRadius: 6).fill(Color.white.opacity(0.08)))
            }
            HStack(spacing: 8) {
                if phase == .allowed {
                    Label(text.allow, systemImage: "checkmark.circle.fill")
                        .labelStyle(.titleAndIcon)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.green)
                        .frame(maxWidth: .infinity)
                        .frame(height: 30)
                        .background(Capsule().fill(Color.green.opacity(0.14)))
                        .transition(.opacity.combined(with: .scale(scale: 0.96)))
                } else {
                    pill(text.deny, key: "⌘N", prominent: false, pressed: false)
                    pill(text.allow, key: "⌘Y", prominent: true, pressed: phase == .pressing)
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 34)
        .padding(.bottom, 12)
        .frame(width: 372, alignment: .leading)
    }

    private func pill(_ title: String, key: String, prominent: Bool, pressed: Bool) -> some View {
        HStack(spacing: 6) {
            Text(title).font(.system(size: 12, weight: .semibold))
            Text(key).font(.system(size: 10, weight: .medium)).opacity(0.6)
        }
        .foregroundStyle(prominent ? Color.black : Color.white)
        .frame(maxWidth: .infinity)
        .frame(height: 30)
        .background(Capsule().fill(prominent ? Color.white : Color.white.opacity(0.12)))
        .scaleEffect(pressed ? 0.93 : 1)
        .brightness(pressed ? -0.12 : 0)
        .animation(.spring(response: 0.22, dampingFraction: 0.6), value: pressed)
    }

    @MainActor
    private func play() async {
        if reduceMotion { phase = .asking; return }
        let steps: [(Phase, Double)] = [(.resting, 0.9), (.asking, 1.9), (.pressing, 0.35), (.allowed, 1.6),
                                        (.closing, 1.0)]
        while !Task.isCancelled {
            for (next, seconds) in steps {
                withAnimation(.spring(response: 0.42, dampingFraction: 0.8)) { phase = next }
                try? await Task.sleep(for: .seconds(seconds))
                if Task.isCancelled { return }
            }
            round += 1
        }
    }
}

// MARK: - Keys

struct OnboardingKeycap: View {
    let keys: String
    let title: String

    var body: some View {
        HStack(spacing: 7) {
            Text(keys)
                .font(.system(size: 12, weight: .semibold, design: .rounded))
                .padding(.horizontal, 7)
                .frame(minWidth: 34, minHeight: 24)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(Color.primary.opacity(0.07))
                        .shadow(color: .black.opacity(0.18), radius: 0, y: 1.5)
                )
                .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.12)))
            Text(title)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}

struct OnboardingSessionKeys: View {
    @ObservedObject private var l10n = L10n.shared
    private var text: OnboardingStrings { FeatureStrings.onboarding(l10n.language) }

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 18) {
                OnboardingKeycap(keys: "⌘Y", title: text.keyAllow)
                OnboardingKeycap(keys: "⌘N", title: text.keyDeny)
                OnboardingKeycap(keys: "⌘T", title: text.keyAlways)
            }
            Text(text.anyApp)
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }
}

// MARK: - Your agents

/// Every agent the island knows, each with what Openbar did about it: the
/// ones found connect by themselves while the step is open, the rest wait
/// greyed out. Claude Code's hooks go in here, since choosing the AI agents
/// setup is choosing them; the others follow their switch as always.
struct OnboardingAgentsStep: View {
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var service = ClaudeSessionService.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var claudePresent: Bool?
    @State private var askedClaude = false
    @State private var appeared = false

    private var text: OnboardingStrings { FeatureStrings.onboarding(l10n.language) }

    enum TileState: Equatable {
        case checking, connecting, connected, ready, missing, failed
    }

    var body: some View {
        VStack(spacing: 22) {
            VStack(spacing: 6) {
                Text(text.agentsTitle)
                    .font(.system(size: 22, weight: .bold))
                Text(text.agentsBody)
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            .padding(.horizontal, 40)

            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 12), count: 3), spacing: 12) {
                ForEach(Array(ClaudeSessionProvider.allCases.enumerated()), id: \.element) { index, provider in
                    tile(provider)
                        .opacity(appeared ? 1 : 0)
                        .offset(y: appeared ? 0 : 14)
                        .animation(reduceMotion ? nil : .spring(response: 0.5, dampingFraction: 0.8)
                            .delay(Double(index) * 0.07), value: appeared)
                }
            }
            .padding(.horizontal, 32)
        }
        .padding(.vertical, 28)
        .onAppear {
            appeared = true
            refresh()
            service.syncAgentHooks()
            findClaude()
        }
        .task {
            // The hooks and Codex's server answer in their own time; the
            // tiles follow for a while, then the person moves on.
            for _ in 0..<16 {
                try? await Task.sleep(for: .seconds(1.5))
                if Task.isCancelled { return }
                refresh()
            }
        }
        .onChange(of: service.hooksInstalled) { _, _ in connectClaude() }
    }

    private func refresh() {
        service.checkHooks()
        service.checkCodex()
        service.checkAgentHooks()
    }

    private func findClaude() {
        let home = FileManager.default.homeDirectoryForCurrentUser
        Task.detached(priority: .userInitiated) {
            let path = ClaudeCLI.shell(home: home).environment["PATH"]
            let found = ClaudeCLI.locate(path: path, home: home) != nil
                || FileManager.default.fileExists(atPath: home.appending(path: ".claude").path)
            await MainActor.run {
                claudePresent = found
                connectClaude()
            }
        }
    }

    /// Once, when Claude Code is here and its settings lack the hooks.
    private func connectClaude() {
        guard claudePresent == true, service.hooksInstalled == false, !askedClaude else { return }
        askedClaude = true
        service.installHooks(true)
    }

    func state(_ provider: ClaudeSessionProvider) -> TileState {
        switch provider {
        case .claude:
            guard let installed = service.hooksInstalled, let present = claudePresent else { return .checking }
            if installed { return .connected }
            if service.hookFailure != nil { return .failed }
            return present ? .connecting : .missing
        case .codex:
            switch service.codexStatus {
            case nil: return .checking
            case .connected?: return .connected
            case .notRunning?: return .ready
            case .notFound?: return .missing
            }
        case .gemini, .copilot, .opencode, .cursor:
            guard let hooks = service.agentHooks?[provider] else { return .checking }
            if service.agentHookFailures[provider] != nil || hooks.state == .unreadable { return .failed }
            if hooks.state == .installed { return .connected }
            return hooks.present ? .connecting : .missing
        }
    }

    private func tile(_ provider: ClaudeSessionProvider) -> some View {
        let state = state(provider)
        let found = state != .missing
        return VStack(spacing: 10) {
            ZStack(alignment: .bottomTrailing) {
                Circle()
                    .fill(Color.black)
                    .frame(width: 52, height: 52)
                    .overlay(NotchSessionAgentGlyph(provider: provider, size: 20,
                                                    working: false))
                if state == .connected || state == .ready {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 17, weight: .semibold))
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white, Color.green)
                        .background(Circle().fill(Color(nsColor: .windowBackgroundColor)).padding(1))
                        .offset(x: 3, y: 3)
                        .transition(.scale(scale: 0.2).combined(with: .opacity))
                }
            }
            .animation(reduceMotion ? nil : .spring(response: 0.38, dampingFraction: 0.55), value: state)
            Text(provider.displayName)
                .font(.system(size: 12.5, weight: .semibold))
                .lineLimit(1)
            status(state, provider: provider)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 16)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(state == .connected ? Color.green.opacity(0.07) : Color.primary.opacity(0.045))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .strokeBorder(state == .connected ? Color.green.opacity(0.28) : Color.primary.opacity(0.06))
        )
        .opacity(found ? 1 : 0.45)
        .saturation(found ? 1 : 0)
        .animation(.easeOut(duration: 0.25), value: state)
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private func status(_ state: TileState, provider: ClaudeSessionProvider) -> some View {
        switch state {
        case .checking:
            ProgressView().controlSize(.small).frame(height: 18)
        case .connecting:
            HStack(spacing: 5) {
                ProgressView().controlSize(.mini)
                Text(text.agentConnecting)
            }
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .frame(height: 18)
        case .connected:
            Text(text.agentConnected).font(.system(size: 11, weight: .medium)).foregroundStyle(.green).frame(height: 18)
        case .ready:
            Text(text.agentReady).font(.system(size: 11, weight: .medium)).foregroundStyle(.green).frame(height: 18)
        case .missing:
            Text(text.agentNotInstalled).font(.system(size: 11)).foregroundStyle(.secondary).frame(height: 18)
        case .failed:
            Button(text.retry) {
                if provider == .claude { service.installHooks(true) } else { service.installAgentHooks(provider, true) }
            }
            .controlSize(.small)
            .help(provider == .claude ? (service.hookFailure ?? text.agentFailed)
                  : (service.agentHookFailures[provider] ?? text.agentFailed))
            .frame(height: 18)
        }
    }
}

// MARK: - What sessions need

/// Only the permissions sessions use: scripting the terminals replies are
/// typed into, and the microphone and speech recognition when voice replies
/// are on. Each is asked from its own button, never all at once.
struct OnboardingSessionPermissionsStep: View {
    @ObservedObject private var l10n = L10n.shared
    @AppStorage(DefaultsKey.notchSessionsVoiceReplies) private var voiceReplies = true
    @State private var terminals: [Terminal] = []
    @State private var voice: VoiceState = Self.voiceState()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var text: OnboardingStrings { FeatureStrings.onboarding(l10n.language) }

    struct Terminal: Identifiable, Equatable {
        let id: String
        let name: String
        let url: URL
        var status: Permissions.AutomationStatus
    }

    enum VoiceState { case granted, undetermined, denied }

    /// The terminals that take a reply through AppleScript.
    private static let scripted: [String] = [ClaudeSessionHosts.terminal, ClaudeTerminalApps.iterm,
                                             ClaudeTerminalApps.ghostty]

    var body: some View {
        VStack(spacing: 22) {
            Image(systemName: "hand.raised.fill")
                .font(.system(size: 26, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: 58, height: 58)
                .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Theme.spaceGradient))
            Text(text.permissionsTitle)
                .font(.system(size: 22, weight: .bold))

            VStack(spacing: 10) {
                card(icon: AnyView(terminalIcons), title: text.terminalsTitle, body: text.terminalsBody) {
                    terminalAction
                }
                if voiceReplies {
                    card(icon: AnyView(Image(systemName: "waveform")
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(Color.brandAccent)),
                         title: text.voiceTitle, body: text.voiceBody) { voiceAction }
                }
            }
            .padding(.horizontal, 32)
        }
        .padding(.vertical, 28)
        .onAppear(perform: refresh)
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in refresh() }
    }

    private var terminalIcons: some View {
        HStack(spacing: -10) {
            ForEach(terminals.prefix(3)) { terminal in
                Image(nsImage: NSWorkspace.shared.icon(forFile: terminal.url.path))
                    .resizable()
                    .frame(width: 30, height: 30)
            }
        }
    }

    private func card(icon: AnyView, title: String, body: String,
                      @ViewBuilder action: () -> some View) -> some View {
        HStack(spacing: 14) {
            icon.frame(width: 52)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 13, weight: .semibold))
                Text(body).font(.system(size: 11.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 8)
            action()
        }
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.primary.opacity(0.05)))
    }

    @ViewBuilder
    private var terminalAction: some View {
        let waiting = terminals.filter { $0.status == .undetermined }
        if terminals.contains(where: { $0.status == .denied }) {
            Button(text.openSettings) { Permissions.shared.openAutomationSettings() }
        } else if !waiting.isEmpty {
            Button(text.allow) { askTerminals(waiting) }
                .buttonStyle(.borderedProminent)
        } else if !terminals.isEmpty, terminals.allSatisfy({ $0.status == .granted }) {
            granted
        } else {
            Text(text.askedLater)
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.trailing)
                .frame(maxWidth: 130, alignment: .trailing)
        }
    }

    @ViewBuilder
    private var voiceAction: some View {
        switch voice {
        case .granted: granted
        case .denied: Button(text.openSettings) { Permissions.shared.openMicrophoneSettings() }
        case .undetermined:
            Button(text.allow) {
                Permissions.shared.requestMicrophone { granted in
                    guard granted else { return voice = Self.voiceState() }
                    SFSpeechRecognizer.requestAuthorization { _ in
                        DispatchQueue.main.async { withAnimation { voice = Self.voiceState() } }
                    }
                }
            }
            .buttonStyle(.borderedProminent)
        }
    }

    private var granted: some View {
        Label(text.allowed, systemImage: "checkmark.circle.fill")
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(.green)
            .transition(.scale.combined(with: .opacity))
    }

    private func refresh() {
        voice = Self.voiceState()
        Task.detached(priority: .userInitiated) {
            let found: [Terminal] = Self.scripted.compactMap { id in
                guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: id) else { return nil }
                let name = FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "")
                return Terminal(id: id, name: name, url: url, status: Permissions.automationStatus(bundleID: id))
            }
            await MainActor.run { withAnimation { terminals = found } }
        }
    }

    /// Asks each terminal that is running; one that is not is asked the
    /// first time a reply goes to it.
    private func askTerminals(_ waiting: [Terminal]) {
        Task.detached(priority: .userInitiated) {
            for terminal in waiting { _ = AppleScriptRunner.consentToAutomate(bundleID: terminal.id) }
            await MainActor.run { refresh() }
        }
    }

    static func voiceState() -> VoiceState {
        let microphone = AVCaptureDevice.authorizationStatus(for: .audio)
        let speech = SFSpeechRecognizer.authorizationStatus()
        if microphone == .authorized && speech == .authorized { return .granted }
        if microphone == .denied || microphone == .restricted || speech == .denied || speech == .restricted {
            return .denied
        }
        return .undetermined
    }
}

// MARK: - Try it

/// Puts a sample request in the real notch, for the person to answer there
/// with a click or a key, then shows how they answered.
struct OnboardingTryStep: View {
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var service = ClaudeSessionService.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false
    @State private var bounce = false

    private var text: OnboardingStrings { FeatureStrings.onboarding(l10n.language) }

    private var answered: Bool? {
        guard shown, let answer = service.sampleAnswer else { return nil }
        if case .deny = answer { return false }
        return true
    }

    var body: some View {
        VStack(spacing: 20) {
            ZStack {
                Circle()
                    .fill(answered == nil ? Color.brandAccent.opacity(0.14)
                          : (answered == true ? Color.green.opacity(0.16) : Color.orange.opacity(0.16)))
                    .frame(width: 96, height: 96)
                    .scaleEffect(bounce ? 1.06 : 1)
                Image(systemName: answered == nil ? "arrow.up" : (answered == true ? "checkmark" : "xmark"))
                    .font(.system(size: 36, weight: .bold))
                    .foregroundStyle(answered == nil ? Color.brandAccent : (answered == true ? Color.green : Color.orange))
                    .offset(y: answered == nil && bounce ? -6 : 0)
                    .contentTransition(.symbolEffect(.replace))
            }
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.9).repeatForever(autoreverses: true), value: bounce)
            .onAppear { bounce = !reduceMotion }

            VStack(spacing: 6) {
                Text(answered == nil ? text.tryTitle : (answered == true ? text.allowed : text.tryDenied))
                    .font(.system(size: 22, weight: .bold))
                    .contentTransition(.opacity)
                Text(answered == nil ? text.tryBody : text.tryResult)
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 44)
            }
            .animation(.easeOut(duration: 0.25), value: answered)

            Button {
                shown = service.showSample()
            } label: {
                Label(shown ? text.tryAgain : text.tryButton, systemImage: "sparkles")
                    .padding(.horizontal, 6)
            }
            .controlSize(.large)
            .buttonStyle(.borderedProminent)

            OnboardingSessionKeys()
                .padding(.top, 8)
        }
        .padding(.vertical, 28)
        .task {
            // The island shows the sample by itself once the step has
            // settled: the point is to see a session working, not to find a
            // button first.
            try? await Task.sleep(for: .milliseconds(700))
            if !Task.isCancelled && !shown { shown = service.showSample() }
        }
        .onDisappear { if shown && service.sampleAnswer == nil { service.dismissSample() } }
    }
}

// MARK: - Done

/// The top of a screen as it now is: the notch in the middle, the helmet in
/// the menu bar, each with a ring pulsing round it and its name under it.
struct OnboardingMenuBarScene: View {
    var showsNotch: Bool
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulse = false

    private var text: OnboardingStrings { FeatureStrings.onboarding(l10n.language) }

    var body: some View {
        ZStack(alignment: .top) {
            Theme.spaceGradient
            RadialGradient(colors: [Color.brandAccent.opacity(0.18), .clear], center: .top,
                           startRadius: 2, endRadius: 260)
            // The menu bar.
            Rectangle()
                .fill(Color.white.opacity(0.07))
                .frame(height: 26)
                .overlay(alignment: .leading) {
                    HStack(spacing: 14) {
                        Image(systemName: "apple.logo")
                        Text("Finder").fontWeight(.semibold)
                        Text("File")
                        Text("Edit")
                    }
                    .font(.system(size: 11))
                    .foregroundStyle(.white.opacity(0.55))
                    .padding(.leading, 14)
                }
                .overlay(alignment: .trailing) {
                    HStack(spacing: 13) {
                        helmet
                        Image(systemName: "wifi")
                        Image(systemName: "battery.75percent")
                        Text("9:41")
                    }
                    .font(.system(size: 11))
                    .foregroundStyle(.white.opacity(0.55))
                    .padding(.trailing, 14)
                }
            if showsNotch {
                UnevenRoundedRectangle(bottomLeadingRadius: 10, bottomTrailingRadius: 10, style: .continuous)
                    .fill(Color.black)
                    .frame(width: 130, height: 26)
                    .overlay(alignment: .leading) {
                        NotchSessionAgentGlyph(provider: .claude, size: 10, working: false).padding(.leading, 9)
                    }
                    .overlay(ring(width: 150, height: 40).offset(y: -2))
                    .overlay(alignment: .bottom) { callout(text.doneNotch).offset(y: 44) }
            }
        }
        .frame(height: 200)
        .clipped()
        .onAppear { pulse = !reduceMotion }
        .accessibilityHidden(true)
    }

    private var helmet: some View {
        Image(nsImage: BlackHoleGlyph.image(active: false) ?? NSImage())
            .renderingMode(.template)
            .foregroundStyle(.white)
            .overlay(ring(width: 34, height: 34))
            .overlay(alignment: .bottom) { callout(text.doneMenuBar).offset(y: 46).fixedSize() }
    }

    private func ring(width: CGFloat, height: CGFloat) -> some View {
        Capsule()
            .strokeBorder(Color.brandAccent, lineWidth: 2)
            .frame(width: width, height: height)
            .scaleEffect(pulse ? 1.12 : 0.96)
            .opacity(pulse ? 0.15 : 0.9)
            .animation(reduceMotion ? nil : .easeOut(duration: 1.4).repeatForever(autoreverses: false), value: pulse)
    }

    private func callout(_ title: String) -> some View {
        VStack(spacing: 4) {
            Image(systemName: "arrow.up")
                .font(.system(size: 11, weight: .bold))
            Text(title)
                .font(.system(size: 11.5, weight: .semibold))
        }
        .foregroundStyle(.white.opacity(0.9))
        .fixedSize()
    }
}
