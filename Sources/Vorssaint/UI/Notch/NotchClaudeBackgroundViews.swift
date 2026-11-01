// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

/// Says a session runs in the background, in words, on its row.
struct NotchClaudeBackgroundMark: View {
    let text: NotchClaudeSessionStrings

    var body: some View {
        Text(text.backgroundMark)
            .font(.system(size: 9.5, weight: .semibold))
            .foregroundStyle(.white.opacity(0.72))
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(.white.opacity(0.1), in: Capsule())
            .help(text.backgroundHelp)
    }
}

/// What can be done with a background session: read its output, stop it,
/// remove it. Stopping and removing ask first, in place.
struct NotchClaudeBackgroundBar: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @Binding var showingLogs: Bool
    @ObservedObject private var service = ClaudeSessionService.shared
    @ObservedObject private var notch = NotchService.shared
    @State private var asking: Action?
    @State private var working = false
    @State private var problem: String?

    private enum Action { case stop, remove }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let asking { confirmation(asking) } else { buttons }
            if let problem {
                Text(problem)
                    .font(.system(size: 10.5))
                    .foregroundStyle(Color(red: 1.0, green: 0.55, blue: 0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var buttons: some View {
        HStack(spacing: 5) {
            NotchClaudeBackgroundMark(text: text)
            Spacer(minLength: 4)
            button(text.logs, symbol: "text.alignleft", active: showingLogs) {
                showingLogs.toggle()
                // Logs read in the enlarged page.
                if showingLogs { notch.setSessionsEnlarged(true) }
            }
            if NotchClaudeSessionsSupport.canStop(session) {
                button(text.stopSession, symbol: "stop.fill") { problem = nil; asking = .stop }
            }
            if NotchClaudeSessionsSupport.canRemove(session) {
                button(text.removeSession, symbol: "trash") { problem = nil; asking = .remove }
            }
        }
    }

    private func confirmation(_ action: Action) -> some View {
        HStack(spacing: 6) {
            Text(action == .stop ? text.stopConfirm : text.removeConfirm)
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.85))
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            button(text.keepSession, symbol: nil) { asking = nil }
            button(action == .stop ? text.stopSession : text.removeSession, symbol: action == .stop ? "stop.fill" : "trash",
                   destructive: true) { run(action) }
                .disabled(working)
        }
    }

    private func run(_ action: Action) {
        working = true
        let finish: (String?) -> Void = { failure in
            working = false
            asking = nil
            problem = failure
        }
        switch action {
        case .stop: service.stopBackground(session, completion: finish)
        case .remove: service.removeBackground(session, completion: finish)
        }
    }

    private func button(_ title: String, symbol: String?, active: Bool = false, destructive: Bool = false,
                        action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let symbol { Image(systemName: symbol).font(.system(size: 10, weight: .semibold)) }
                Text(title).font(.system(size: 11.5, weight: .medium)).lineLimit(1)
            }
            .foregroundStyle(destructive ? Color(red: 1.0, green: 0.55, blue: 0.5) : .white.opacity(active ? 1 : 0.85))
            .padding(.horizontal, 8)
            .frame(height: 26)
            .background(.white.opacity(active ? 0.16 : 0.07), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
    }
}

/// A background session's recent output, read again while it shows.
struct NotchClaudeBackgroundLogsView: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var logs: String?
    @State private var loaded = false

    private let timer = Timer.publish(every: 5, on: .main, in: .common).autoconnect()

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(text.logsTitle)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
            ScrollViewReader { reader in
                ScrollView {
                    Group {
                        if let logs, !logs.isEmpty {
                            Text(logs)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(.white.opacity(0.88))
                                .textSelection(.enabled)
                        } else if loaded {
                            Text(logs == nil ? text.logsGone : text.logsEmpty)
                                .font(.system(size: 12))
                                .foregroundStyle(.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                    Color.clear.frame(height: 1).id("end")
                }
                .background(.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .onChange(of: logs) { _, _ in reader.scrollTo("end", anchor: .bottom) }
            }
        }
        .onAppear(perform: load)
        .onReceive(timer) { _ in if session.background?.ended != true { load() } }
    }

    private func load() {
        service.backgroundLogs(session) { screen in
            logs = screen
            loaded = true
        }
    }
}

/// Starts a session: which folder, which agent and model, optionally what to
/// say first, and whether it runs in the background or in a terminal.
struct NotchClaudeNewSessionView: View {
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var folders: [ClaudeRecentFolder] = []
    @State private var chosen: ClaudeRecentFolder?
    @State private var prompt = ""
    @State private var inBackground = true
    @State private var starting = false
    @State private var problem: String?
    @State private var claudeModels: [NewSessionModel] = []
    @State private var codexModels: [NewSessionModel] = []
    @AppStorage(DefaultsKey.notchNewSessionAgent) private var agentChoice = ClaudeSessionProvider.claude.rawValue
    @AppStorage(DefaultsKey.notchNewSessionClaudeModel) private var claudeModel = ""
    @AppStorage(DefaultsKey.notchNewSessionCodexModel) private var codexModel = ""
    @FocusState private var promptFocused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Namespace private var agentPill

    /// The agents this Mac can start: Claude Code, Codex, or both.
    private var agents: [ClaudeSessionProvider] {
        (service.canStartSessions ? [.claude] : []) + (service.canStartCodex ? [.codex] : [])
    }

    private var agent: ClaudeSessionProvider {
        let picked = ClaudeSessionProvider(rawValue: agentChoice) ?? .claude
        return agents.contains(picked) ? picked : (agents.first ?? .claude)
    }

    /// Codex runs threads in its own background server, so it needs no Agent View.
    private var canRunInBackground: Bool { agent == .codex || service.backgroundAvailable }
    private var background: Bool { inBackground && canRunInBackground }

    private var models: [NewSessionModel] { agent == .codex ? codexModels : claudeModels }
    private var modelKey: Binding<String> { agent == .codex ? $codexModel : $claudeModel }
    private var model: NewSessionModel? {
        models.first { $0.key == modelKey.wrappedValue } ?? models.first
    }

    /// In the background a Codex thread starts with its first turn.
    private var needsPrompt: Bool {
        agent == .codex && background && prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var canStart: Bool { chosen != nil && !starting && !needsPrompt }

    /// Springs for things that move, a quick fade when motion is reduced.
    private var motion: Animation {
        reduceMotion ? .easeOut(duration: 0.12) : .spring(response: 0.32, dampingFraction: 0.82)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            folderList
            TextField(text.firstPrompt, text: $prompt)
                .textFieldStyle(.plain)
                .font(.system(size: 12))
                .focused($promptFocused)
                .padding(.horizontal, 10)
                .frame(height: 30)
                .background(.white.opacity(promptFocused ? 0.11 : 0.08), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .strokeBorder(agent.tint.opacity(promptFocused ? 0.55 : 0), lineWidth: 1)
                )
                .animation(.easeOut(duration: 0.18), value: promptFocused)
                .animation(.easeOut(duration: 0.25), value: agent)
                .accessibilityLabel(text.firstPrompt)
            if let message = problem ?? (needsPrompt ? text.codexNeedsPrompt : nil) {
                Text(message)
                    .font(.system(size: 10.5))
                    .foregroundStyle(problem == nil ? Color.white.opacity(0.6) : Color(red: 1.0, green: 0.55, blue: 0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .id(message)
                    .transition(.opacity.combined(with: .offset(y: -3)))
            }
            HStack(spacing: 6) {
                if agents.count > 1 { agentPicker }
                Spacer(minLength: 4)
                if !models.isEmpty { modelMenu }
            }
            HStack(spacing: 6) {
                if canRunInBackground {
                    mode(text.runInBackground, selected: inBackground) { inBackground = true }
                    mode(text.runInTerminal, selected: !inBackground) { inBackground = false }
                } else {
                    Text(text.backgroundOffHelp)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 4)
                startButton
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(motion, value: agent)
        .animation(motion, value: needsPrompt)
        .animation(motion, value: problem)
        .notchBand(leading: {
            NotchBandBackButton(title: text.newSession) { service.composingSession = false }
        })
        .onAppear {
            service.recentFolders { found in
                folders = found
                if chosen == nil { chosen = found.first }
                loadModels()
            }
        }
        .onChange(of: chosen?.configHome) { _, _ in loadModels() }
    }

    private func loadModels() {
        service.newSessionModels(configHome: chosen?.configHome) { claude, codex in
            claudeModels = claude
            codexModels = codex
        }
    }

    /// Claude or Codex, the pill sliding to the one picked.
    private var agentPicker: some View {
        HStack(spacing: 2) {
            ForEach(agents, id: \.self) { provider in
                let selected = provider == agent
                Button {
                    guard provider != agent else { return }
                    problem = nil
                    withAnimation(motion) { agentChoice = provider.rawValue }
                } label: {
                    HStack(spacing: 5) {
                        NotchSessionAgentGlyph(provider: provider, size: provider == .claude ? 11 : 10, working: false)
                            .frame(width: 13, height: 13)
                            .saturation(selected ? 1 : 0)
                            .opacity(selected ? 1 : 0.55)
                            .scaleEffect(selected && !reduceMotion ? 1.06 : 1)
                        Text(provider == .claude ? "Claude" : "Codex")
                            .font(.system(size: 11.5, weight: .medium))
                            .foregroundStyle(.white.opacity(selected ? 1 : 0.6))
                    }
                    .padding(.horizontal, 9)
                    .frame(height: 26)
                    .background {
                        if selected {
                            RoundedRectangle(cornerRadius: 7, style: .continuous)
                                .fill(provider.tint.opacity(0.22))
                                .overlay(RoundedRectangle(cornerRadius: 7, style: .continuous)
                                    .strokeBorder(provider.tint.opacity(0.45), lineWidth: 1))
                                .matchedGeometryEffect(id: "pill", in: agentPill)
                        }
                    }
                    .contentShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 7, highlights: !selected))
                .accessibilityLabel(provider == .claude ? "Claude Code" : "Codex")
                .accessibilityAddTraits(selected ? .isSelected : [])
            }
        }
        .padding(2)
        .background(.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(text.agentPicker)
    }

    /// The model, picked from a menu: Codex lists more than fit as chips.
    private var modelMenu: some View {
        Menu {
            Picker(text.modelPicker, selection: modelKey) {
                ForEach(models) { option in
                    if let detail = option.detail {
                        Text("\(option.name) · \(detail)").tag(option.key)
                    } else {
                        Text(option.name).tag(option.key)
                    }
                }
            }
            .pickerStyle(.inline)
            .labelsHidden()
        } label: {
            HStack(spacing: 5) {
                Image(systemName: "cpu")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.5))
                ZStack(alignment: .leading) {
                    Text(modelLabel)
                        .font(.system(size: 11.5, weight: .medium))
                        .foregroundStyle(.white.opacity(0.9))
                        .lineLimit(1)
                        .id("\(agent.rawValue)-\(modelLabel)")
                        .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: 5)),
                                                removal: .opacity.combined(with: .offset(y: -5))))
                }
                .clipped()
                Image(systemName: "chevron.up.chevron.down")
                    .font(.system(size: 8.5, weight: .bold))
                    .foregroundStyle(.white.opacity(0.45))
            }
            .padding(.horizontal, 9)
            .frame(height: 28)
            .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .menuStyle(.button)
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .menuIndicator(.hidden)
        .fixedSize()
        .animation(motion, value: modelLabel)
        .help(text.modelPicker)
        .accessibilityLabel("\(text.modelPicker): \(modelLabel)")
    }

    /// The model's name; for the default, which model that is, when known.
    private var modelLabel: String {
        guard let model else { return text.defaultModel }
        if model.id == nil, let detail = model.detail { return detail }
        return model.name
    }

    private var folderList: some View {
        ScrollView {
            LazyVStack(spacing: 3) {
                if folders.isEmpty {
                    Text(text.noRecentFolders)
                        .font(.system(size: 11.5))
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading)
                        .padding(.horizontal, 9)
                }
                ForEach(folders) { folder in
                    Button { chosen = folder; problem = nil } label: {
                        HStack(spacing: 7) {
                            Image(systemName: chosen == folder ? "checkmark.circle.fill" : "folder")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(chosen == folder ? agent.tint : .white.opacity(0.5))
                                .frame(width: 14)
                            Text(folder.name)
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.white.opacity(0.92))
                                .lineLimit(1)
                            Text(folder.path)
                                .font(.system(size: 10))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                                .truncationMode(.head)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 9)
                        .frame(height: 28)
                        .background(.white.opacity(chosen == folder ? 0.1 : 0), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                        .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    }
                    .buttonStyle(NotchButtonStyle(cornerRadius: 8, lifts: false))
                    .accessibilityLabel(folder.path)
                    .accessibilityAddTraits(chosen == folder ? .isSelected : [])
                }
                Button(action: browse) {
                    HStack(spacing: 7) {
                        Image(systemName: "folder.badge.plus").font(.system(size: 11, weight: .semibold)).frame(width: 14)
                        Text(text.browseFolder).font(.system(size: 12, weight: .medium))
                        Spacer(minLength: 0)
                    }
                    .foregroundStyle(.white.opacity(0.75))
                    .padding(.horizontal, 9)
                    .frame(height: 28)
                    .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 8, lifts: false))
            }
        }
        .scrollIndicators(.automatic)
    }

    private func mode(_ title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button { withAnimation(motion) { action() } } label: {
            Text(title)
                .font(.system(size: 11.5, weight: .medium))
                .foregroundStyle(.white.opacity(selected ? 1 : 0.6))
                .padding(.horizontal, 10)
                .frame(height: 28)
                .background(.white.opacity(selected ? 0.16 : 0.05), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var startButton: some View {
        Button(action: start) {
            ZStack {
                Text(text.startSession)
                    .font(.system(size: 12, weight: .semibold))
                    .opacity(starting ? 0 : 1)
                if starting {
                    ProgressView()
                        .controlSize(.small)
                        .scaleEffect(0.7)
                        .tint(.white)
                        .transition(.opacity.combined(with: .scale(scale: 0.6)))
                }
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 14)
            .frame(height: 28)
            .background(agent.tint.opacity(canStart || starting ? 0.85 : 0.3),
                        in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .animation(motion, value: starting)
            .animation(.easeOut(duration: 0.25), value: canStart)
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .disabled(!canStart)
    }

    private func start() {
        guard let chosen, canStart else { return }
        starting = true
        problem = nil
        let picked = model?.id
        service.startSession(in: chosen.path, configHome: chosen.configHome, prompt: prompt, background: background,
                             provider: agent, model: picked) { failure in
            starting = false
            problem = failure
            if failure == nil { service.composingSession = false }
        }
    }

    /// A folder from anywhere, kept in the list for this visit.
    private func browse() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.level = .modalPanel
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let path = url.standardizedFileURL.path
        let existing = folders.first { $0.path == path }
        let folder = existing ?? ClaudeRecentFolder(path: path, configHome: folders.first?.configHome, used: nil)
        if existing == nil { folders.insert(folder, at: 0) }
        chosen = folder
        problem = nil
    }
}
