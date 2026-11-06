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

    private enum Action { case stop, remove, removeAnyway }

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
            Text(action == .stop ? text.stopConfirm : action == .remove ? text.removeConfirm : text.worktreeUnsaved)
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.85))
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            button(text.keepSession, symbol: nil) { asking = nil }
            button(action == .stop ? text.stopSession : action == .remove ? text.removeSession : text.removeAnyway,
                   symbol: action == .stop ? "stop.fill" : "trash", destructive: true) { run(action) }
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
        let removed: (WorktreeRemoval) -> Void = { outcome in
            switch outcome {
            case .done: finish(nil)
            case .failed(let said): finish(said)
            // Its worktree holds work found nowhere else: ask again, in words that say so.
            case .unsaved:
                working = false
                asking = .removeAnyway
            }
        }
        switch action {
        case .stop: service.stopBackground(session, completion: finish)
        case .remove: service.removeBackground(session, completion: removed)
        case .removeAnyway: service.removeBackground(session, anyway: true, completion: removed)
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

/// What can be done with the worktree a session works in: open its folder,
/// copy its branch, and remove one the island made once the session is done
/// with it. Claude Code's own go with `claude rm`.
struct NotchClaudeWorktreeBar: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var branch: String?
    @State private var copied = false
    @State private var asking: Ask?
    @State private var working = false
    @State private var problem: String?

    private enum Ask { case remove, unsaved }

    private var canRemove: Bool {
        SessionWorktrees.isOwn(session.cwd) && !session.phase.isActive && !session.phase.isWaiting
    }

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
        .onAppear { service.worktreeBranch(session) { branch = $0 } }
    }

    private var buttons: some View {
        HStack(spacing: 5) {
            Label(session.worktree ?? "", systemImage: "arrow.triangle.branch")
                .labelStyle(NotchClaudeSessionInlineLabel())
                .font(.system(size: 10.5, weight: .medium))
                .foregroundStyle(.white.opacity(0.6))
                .lineLimit(1)
                .truncationMode(.middle)
                .help(text.worktreeMark)
            Spacer(minLength: 4)
            chip(text.openWorktree, symbol: "folder") {
                guard let folder = SessionWorktrees.worktreeFolder(session.cwd) else { return }
                NSWorkspace.shared.open(URL(fileURLWithPath: folder, isDirectory: true))
            }
            if let branch {
                chip(copied ? text.branchCopied : text.copyBranch, symbol: copied ? "checkmark" : "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(branch, forType: .string)
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
                }
                .help(branch)
            }
            if canRemove {
                chip(text.removeWorktree, symbol: "trash") { problem = nil; asking = .remove }
            }
        }
    }

    private func confirmation(_ ask: Ask) -> some View {
        HStack(spacing: 6) {
            Text(ask == .remove ? text.removeWorktreeConfirm : text.worktreeUnsaved)
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.85))
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            chip(text.keepSession, symbol: nil) { asking = nil }
            chip(ask == .remove ? text.removeWorktree : text.removeAnyway, symbol: "trash", destructive: true) {
                remove(anyway: ask == .unsaved)
            }
            .disabled(working)
        }
    }

    private func remove(anyway: Bool) {
        working = true
        service.removeWorktree(session, anyway: anyway) { outcome in
            working = false
            switch outcome {
            case .done: asking = nil
            case .unsaved: asking = .unsaved
            case .failed(let said):
                asking = nil
                problem = said
            }
        }
    }

    private func chip(_ title: String, symbol: String?, destructive: Bool = false,
                      action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let symbol {
                    Image(systemName: symbol)
                        .font(.system(size: 10, weight: .semibold))
                        .contentTransition(.symbolEffect(.replace))
                }
                Text(title).font(.system(size: 11.5, weight: .medium)).lineLimit(1)
            }
            .foregroundStyle(destructive ? Color(red: 1.0, green: 0.55, blue: 0.5) : .white.opacity(0.85))
            .padding(.horizontal, 8)
            .frame(height: 26)
            .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
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
    @AppStorage(DefaultsKey.notchNewSessionNoWorktree) private var noWorktree = ""
    @State private var worktreeName = ""
    /// What the folder picked is to git; nil while it's being asked.
    @State private var repository: SessionWorktrees.RepoState?
    @FocusState private var promptFocused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Namespace private var agentPill
    @Namespace private var folderHighlight

    /// The agents this Mac can start: Claude Code, Codex, and those of
    /// Gemini CLI, Copilot CLI, OpenCode and `cursor-agent` that are
    /// installed and shown in Sessions.
    private var agents: [ClaudeSessionProvider] {
        let shown = NotchClaudeSessionsSupport.shownAgents()
        return (service.canStartSessions ? [.claude] : []) + (service.canStartCodex ? [.codex] : [])
            + ClaudeSessionProvider.hookAgents.filter { service.startableAgents.contains($0) && shown.contains($0) }
    }

    /// Past three agents only the one picked is named; the rest show their mark.
    private var compactPicker: Bool { agents.count > 3 }

    private var agent: ClaudeSessionProvider {
        let picked = ClaudeSessionProvider(rawValue: agentChoice) ?? .claude
        return agents.contains(picked) ? picked : (agents.first ?? .claude)
    }

    /// Codex runs threads in its own background server, so it needs no Agent
    /// View; the agents after it run only in a terminal.
    private var canRunInBackground: Bool {
        agent == .codex || (agent == .claude && service.backgroundAvailable)
    }
    private var background: Bool { inBackground && canRunInBackground }

    private var models: [NewSessionModel] {
        switch agent {
        case .codex: return codexModels
        case .claude: return claudeModels
        default: return []
        }
    }
    private var modelKey: Binding<String> { agent == .codex ? $codexModel : $claudeModel }
    private var model: NewSessionModel? {
        models.first { $0.key == modelKey.wrappedValue } ?? models.first
    }

    /// A session in the background has nobody to type to it, so it starts with a prompt.
    private var needsPrompt: Bool {
        background && prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var promptPlaceholder: String { background ? text.firstPromptRequired : text.firstPrompt }

    /// Whether this agent starts in a worktree of its own, as last chosen for
    /// it; on until turned off.
    private var wantsWorktree: Bool { !noWorktree.split(separator: ",").contains(Substring(agent.rawValue)) }
    private var isRepository: Bool { repository?.root != nil }
    private var usesWorktree: Bool { wantsWorktree && isRepository }

    private func setWantsWorktree(_ on: Bool) {
        var off = Set(noWorktree.split(separator: ",").map(String.init))
        if on { off.remove(agent.rawValue) } else { off.insert(agent.rawValue) }
        noWorktree = off.sorted().joined(separator: ",")
    }

    /// The name the worktree gets when none is typed.
    private var worktreePlaceholder: String {
        let slug = SessionWorktrees.slug(prompt)
        return slug.isEmpty ? text.worktreeNamePlaceholder : slug
    }

    /// What the worktree switch means for the folder picked.
    private var worktreeNote: String? {
        switch repository {
        case .none: return nil
        case .notRepo?: return text.worktreeNotRepo
        case .dirty?: return wantsWorktree ? text.worktreeFromHead : nil
        case .clean?: return nil
        }
    }

    private var canStart: Bool { chosen != nil && !starting && !needsPrompt }

    /// Springs for things that move, a quick fade when motion is reduced.
    private var motion: Animation {
        reduceMotion ? .easeOut(duration: 0.12) : .spring(response: 0.32, dampingFraction: 0.82)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            folderList
            TextField(promptPlaceholder, text: $prompt)
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
                .accessibilityLabel(promptPlaceholder)
            if let message = problem ?? (needsPrompt ? text.backgroundNeedsPrompt : nil) {
                Text(message)
                    .font(.system(size: 10.5))
                    .foregroundStyle(problem == nil ? Color.white.opacity(0.6) : Color(red: 1.0, green: 0.55, blue: 0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .id(message)
                    .transition(.opacity.combined(with: .offset(y: -3)))
            }
            worktreeRow
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
                    Text(agent.reportsThroughHooks ? text.opensInTerminal(agent.displayName) : text.backgroundOffHelp)
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
        .animation(motion, value: usesWorktree)
        .animation(motion, value: worktreeNote)
        .notchBand(leading: {
            NotchBandBackButton(title: text.newSession) { service.composingSession = false }
        })
        .onAppear {
            service.recentFolders { found in
                folders = found
                if chosen == nil { chosen = found.first }
                loadModels()
                checkRepository()
            }
        }
        .onChange(of: chosen?.configHome) { _, _ in loadModels() }
        .onChange(of: chosen?.path) { _, _ in checkRepository() }
    }

    private func checkRepository() {
        guard let path = chosen?.path else { repository = nil; return }
        repository = nil
        service.repositoryState(of: path) { state in
            // An answer for a folder no longer picked is old news.
            if chosen?.path == path { repository = state }
        }
    }

    /// The switch for a worktree of the session's own, its name, and what
    /// it means for this folder.
    private var worktreeRow: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Button {
                    withAnimation(motion) { setWantsWorktree(!wantsWorktree) }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: usesWorktree ? "checkmark.square.fill" : "square")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(usesWorktree ? agent.tint : .white.opacity(0.5))
                            .contentTransition(reduceMotion ? .identity : .symbolEffect(.replace))
                        Image(systemName: "arrow.triangle.branch")
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundStyle(.white.opacity(0.55))
                        Text(text.ownWorktree)
                            .font(.system(size: 11.5, weight: .medium))
                            .foregroundStyle(.white.opacity(isRepository ? 0.9 : 0.45))
                            .lineLimit(1)
                            .fixedSize()
                    }
                    .padding(.horizontal, 8)
                    .frame(height: 26)
                    .background(.white.opacity(usesWorktree ? 0.1 : 0.05), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 8))
                .disabled(!isRepository)
                .help("\(text.ownWorktreeHelp) \(text.worktreeIgnoredFiles)")
                .accessibilityLabel(text.ownWorktree)
                .accessibilityValue(usesWorktree ? "1" : "0")
                .accessibilityAddTraits(usesWorktree ? .isSelected : [])
                if usesWorktree {
                    TextField(worktreePlaceholder, text: $worktreeName)
                        .textFieldStyle(.plain)
                        .font(.system(size: 11.5, design: .monospaced))
                        .padding(.horizontal, 8)
                        .frame(height: 26)
                        .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                        .accessibilityLabel(text.worktreeNamePlaceholder)
                        .transition(.opacity.combined(with: .offset(x: -6)))
                } else {
                    Spacer(minLength: 0)
                }
            }
            if let note = worktreeNote {
                Text(note)
                    .font(.system(size: 10.5))
                    .foregroundStyle(.white.opacity(0.55))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .id(note)
                    .transition(.opacity.combined(with: .offset(y: -3)))
            }
        }
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
                        if selected || !compactPicker {
                            Text(provider.shortName)
                                .font(.system(size: 11.5, weight: .medium))
                                .foregroundStyle(.white.opacity(selected ? 1 : 0.6))
                                .lineLimit(1)
                                .fixedSize()
                                .transition(.opacity)
                        }
                    }
                    .padding(.horizontal, selected || !compactPicker ? 9 : 7)
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
                .accessibilityLabel(provider.displayName)
                .help(provider.displayName)
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
                    Button {
                        guard chosen != folder else { return }
                        problem = nil
                        withAnimation(motion) { chosen = folder }
                    } label: {
                        HStack(spacing: 7) {
                            Image(systemName: chosen == folder ? "checkmark.circle.fill" : "folder")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(chosen == folder ? agent.tint : .white.opacity(0.5))
                                .contentTransition(reduceMotion ? .identity : .symbolEffect(.replace.downUp))
                                .frame(width: 14)
                            Text(folder.name)
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.white.opacity(chosen == folder ? 1 : 0.8))
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
                        .background {
                            // One highlight, sliding to the folder picked.
                            if chosen == folder {
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .fill(.white.opacity(0.1))
                                    .matchedGeometryEffect(id: "folder", in: folderHighlight)
                            }
                        }
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
                             provider: agent, model: picked, worktree: usesWorktree,
                             worktreeName: worktreeName) { failure in
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
        withAnimation(motion) { chosen = folder }
        problem = nil
    }
}
