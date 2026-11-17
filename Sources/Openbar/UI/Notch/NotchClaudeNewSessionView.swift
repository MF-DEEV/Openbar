// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

/// Starts a session in four steps (#270): the project, the agent with its
/// account and model, the options (worktree, safe commands, background or
/// terminal), and the first prompt. The steps sit in the band at the top,
/// Start works from any of them, and each project remembers what it last
/// started with, so a repeat session is: pick the project, type, Start.
struct NotchClaudeNewSessionView: View {
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared

    enum Step: Int, CaseIterable, Comparable {
        case project, agent, options, prompt
        static func < (a: Step, b: Step) -> Bool { a.rawValue < b.rawValue }
    }

    @State private var step: Step = .project
    /// Which way the steps slide: forward pushes the page to the left.
    @State private var forward = true
    @State private var folders: [ClaudeRecentFolder] = []
    @State private var chosen: ClaudeRecentFolder?
    @State private var prompt = ""
    @State private var starting = false
    @State private var problem: String?
    @State private var claudeModels: [NewSessionModel] = []
    @State private var codexModels: [NewSessionModel] = []
    // The last choices of all projects, for a project with none of its own.
    @AppStorage(DefaultsKey.notchNewSessionAgent) private var lastAgent = ClaudeSessionProvider.claude.rawValue
    @AppStorage(DefaultsKey.notchNewSessionClaudeModel) private var lastClaudeModel = ""
    @AppStorage(DefaultsKey.notchNewSessionCodexModel) private var lastCodexModel = ""
    @AppStorage(DefaultsKey.notchNewSessionNoWorktree) private var noWorktree = ""
    @AppStorage(DefaultsKey.notchSessionsLastAccount) private var lastAccount = ""
    // This project's choices.
    @State private var agentChoice = ""
    @State private var modelChoices: [String: String] = [:]
    /// The Claude account picked for this session; nil follows the last one used.
    @State private var accountChoice: String?
    @State private var worktreeChoice: Bool?
    @State private var inBackground = true
    @State private var safeOn = true
    @State private var suggested: [SafeCommands.Entry] = []
    @State private var safeOff: Set<String> = []
    @State private var safeCustom: [String] = []
    @State private var adding = false
    @State private var newCommand = ""
    @State private var commandRefused = false
    @State private var worktreeName = ""
    /// What the folder picked is to git; nil while it's being asked.
    @State private var repository: SessionWorktrees.RepoState?
    @FocusState private var promptFocused: Bool
    @FocusState private var commandFocused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Namespace private var stepPill
    @Namespace private var agentPill
    @Namespace private var folderHighlight

    // MARK: Choices

    /// The agents this Mac can start: Claude Code, Codex, and the others
    /// that are installed and shown in Sessions.
    private var agents: [ClaudeSessionProvider] {
        let shown = NotchClaudeSessionsSupport.shownAgents()
        return (service.canStartSessions ? [.claude] : []) + (service.canStartCodex ? [.codex] : [])
            + ClaudeSessionProvider.hookAgents.filter { service.startableAgents.contains($0) && shown.contains($0) }
    }

    private var agent: ClaudeSessionProvider {
        let picked = ClaudeSessionProvider(rawValue: agentChoice.isEmpty ? lastAgent : agentChoice) ?? .claude
        return agents.contains(picked) ? picked : (agents.first ?? .claude)
    }

    /// Codex runs threads in its own background server, so it needs no Agent
    /// View; the agents after it run only in a terminal.
    private var canRunInBackground: Bool {
        agent == .codex || (agent == .claude && service.backgroundAvailable)
    }
    private var background: Bool { inBackground && canRunInBackground }

    private var shownAccounts: [ClaudeAccount] { service.accounts.filter { !$0.hidden } }
    private var picksAccount: Bool { agent == .claude && shownAccounts.count > 1 }

    private var account: ClaudeAccount? {
        if let accountChoice, let picked = shownAccounts.first(where: { $0.id == accountChoice }) { return picked }
        return ClaudeAccounts.preferred(service.accounts, last: lastAccount.isEmpty ? nil : lastAccount,
                                        folder: chosen?.configHome)
    }

    /// The config folder the session starts with: the account picked while
    /// there is a choice, else the folder's own.
    private var configHome: String? {
        shownAccounts.count > 1 ? account?.path : chosen?.configHome
    }

    private var models: [NewSessionModel] {
        switch agent {
        case .codex: return codexModels
        case .claude: return claudeModels
        default: return []
        }
    }

    private var modelKey: Binding<String> {
        Binding(get: { modelChoices[agent.rawValue] ?? (agent == .codex ? lastCodexModel : lastClaudeModel) },
                set: { modelChoices[agent.rawValue] = $0 })
    }

    private var model: NewSessionModel? {
        models.first { $0.key == modelKey.wrappedValue } ?? models.first
    }

    private var modelLabel: String {
        guard let model else { return text.defaultModel }
        if model.id == nil, let detail = model.detail { return detail }
        return model.name
    }

    private var trimmedPrompt: String { prompt.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// A session in the background has nobody to type to it, so it starts with a prompt.
    private var needsPrompt: Bool { background && trimmedPrompt.isEmpty }

    private var wantsWorktree: Bool {
        worktreeChoice ?? !noWorktree.split(separator: ",").contains(Substring(agent.rawValue))
    }
    private var isRepository: Bool { repository?.root != nil }
    private var usesWorktree: Bool { wantsWorktree && isRepository }

    private var worktreePlaceholder: String {
        let slug = SessionWorktrees.slug(prompt)
        return slug.isEmpty ? text.worktreeNamePlaceholder : slug
    }

    private var worktreeNote: String? {
        switch repository {
        case .none: return nil
        case .notRepo?: return text.worktreeNotRepo
        case .dirty?: return wantsWorktree ? text.worktreeFromHead : nil
        case .clean?: return nil
        }
    }

    private var offersSafeCommands: Bool { SafeCommands.supports(agent) }

    /// What the project suggests; git's three only in a repository.
    private var shownSuggested: [SafeCommands.Entry] {
        repository == .notRepo ? suggested.filter { $0.kind != .git } : suggested
    }

    /// The commands the session starts allowed to run.
    private var safeList: [String] {
        guard safeOn, offersSafeCommands else { return [] }
        return shownSuggested.map(\.command).filter { !safeOff.contains($0) } + safeCustom
    }

    private var canStart: Bool { chosen != nil && !starting }

    private var motion: Animation {
        reduceMotion ? .easeOut(duration: 0.12) : .spring(response: 0.34, dampingFraction: 0.84)
    }

    private var slide: AnyTransition {
        guard !reduceMotion else { return .opacity }
        return .asymmetric(insertion: .move(edge: forward ? .trailing : .leading).combined(with: .opacity),
                           removal: .move(edge: forward ? .leading : .trailing).combined(with: .opacity))
    }

    // MARK: Body

    var body: some View {
        VStack(spacing: 10) {
            stepBar
                .frame(maxWidth: .infinity)
            ZStack {
                page(step)
                    .id(step)
                    .transition(slide)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .clipped()
            footer
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(motion, value: agent)
        .animation(motion, value: problem)
        .animation(motion, value: usesWorktree)
        .animation(motion, value: worktreeNote)
        .animation(motion, value: safeOn)
        .notchBand(leading: {
            NotchBandBackButton(title: text.newSession) { service.composingSession = false }
        })
        .onAppear {
            // An account added since the island opened is offered too.
            service.checkHooks()
            service.recentFolders { found in
                folders = found
                if chosen == nil, let first = found.first { pick(first) }
                loadModels()
            }
        }
        .onChange(of: configHome) { _, _ in loadModels() }
        .onExitCommand { if adding { cancelAdding() } }
    }

    @ViewBuilder
    private func page(_ step: Step) -> some View {
        switch step {
        case .project: centered(width: 560) { folderList }
        case .agent: centered(width: 560) { agentPage }
        case .options: centered(width: 560) { optionsPage }
        case .prompt: centered(width: 560) { promptPage }
        }
    }

    /// A step's content in the middle of the page, no wider than `width`:
    /// as it is when it fits, scrolling when it doesn't.
    private func centered<Content: View>(width: CGFloat, @ViewBuilder _ content: () -> Content) -> some View {
        let content = content().frame(maxWidth: width)
        return ViewThatFits(in: .vertical) {
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            ScrollView {
                content
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 2)
            }
            .scrollIndicators(.automatic)
        }
    }

    /// Moves to a step, sliding the way it lies.
    private func go(_ target: Step) {
        guard target != step else { return }
        if adding { cancelAdding() }
        forward = target > step
        // The page leaving takes the new direction before it goes.
        DispatchQueue.main.async {
            withAnimation(motion) { step = target }
            if target == .prompt {
                DispatchQueue.main.asyncAfter(deadline: .now() + (reduceMotion ? 0.05 : 0.25)) { promptFocused = true }
            }
        }
    }

    // MARK: Step bar

    private func symbol(_ step: Step) -> String {
        switch step {
        case .project: return chosen == nil ? "folder" : "folder.fill"
        case .agent: return "sparkles"
        case .options: return "slider.horizontal.3"
        case .prompt: return "text.bubble"
        }
    }

    private func title(_ step: Step) -> String {
        switch step {
        case .project: return text.stepProject
        case .agent: return text.stepAgent
        case .options: return text.stepOptions
        case .prompt: return text.stepPrompt
        }
    }

    /// The four steps across the top, each with its mark and name, the
    /// current one filled in the agent's colour; a click goes there.
    private var stepBar: some View {
        HStack(spacing: 4) {
            ForEach(Step.allCases, id: \.self) { item in
                let current = item == step
                Button { go(item) } label: {
                    HStack(spacing: 6) {
                        Group {
                            if item == .agent {
                                NotchSessionAgentGlyph(provider: agent, size: agent == .claude ? 13 : 12, working: false)
                                    .saturation(current ? 1 : 0)
                            } else {
                                Image(systemName: symbol(item))
                                    .font(.system(size: 12.5, weight: .semibold))
                                    .contentTransition(reduceMotion ? .identity : .symbolEffect(.replace))
                            }
                        }
                        .frame(width: 16, height: 16)
                        Text(title(item))
                            .font(.system(size: 12.5, weight: current ? .semibold : .medium))
                            .lineLimit(1)
                            .fixedSize()
                    }
                    .foregroundStyle(.white.opacity(current ? 1 : item < step ? 0.75 : 0.45))
                    .padding(.horizontal, 12)
                    .frame(height: 32)
                    .background {
                        if current {
                            Capsule(style: .continuous)
                                .fill(agent.tint.opacity(0.24))
                                .overlay(Capsule(style: .continuous).strokeBorder(agent.tint.opacity(0.5), lineWidth: 1))
                                .matchedGeometryEffect(id: "step", in: stepPill)
                        }
                    }
                    .overlay(alignment: .topTrailing) {
                        // The prompt a background session still needs.
                        if item == .prompt, needsPrompt, !current {
                            Circle().fill(agent.tint).frame(width: 6, height: 6).offset(x: -4, y: 4)
                                .transition(.scale.combined(with: .opacity))
                        }
                    }
                    .contentShape(Capsule(style: .continuous))
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 16, highlights: !current))
                .help(title(item))
                .accessibilityLabel(text.stepLabel(item.rawValue + 1, of: Step.allCases.count, title(item)))
                .accessibilityAddTraits(current ? .isSelected : [])
                if item != .prompt {
                    Capsule()
                        .fill(item < step ? agent.tint.opacity(0.6) : .white.opacity(0.14))
                        .frame(width: 14, height: 2)
                        .accessibilityHidden(true)
                }
            }
        }
        .animation(motion, value: step)
        .animation(motion, value: needsPrompt)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(text.stepsLabel)
    }

    // MARK: 1 · Project

    private var folderList: some View {
        VStack(spacing: 3) {
                if folders.isEmpty {
                    Label(text.noRecentFolders, systemImage: "tray")
                        .font(.system(size: 11.5))
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading)
                        .padding(.horizontal, 9)
                }
                ForEach(folders) { folder in
                    let picked = chosen == folder
                    let known = NewSessionChoiceStore.load(folder.path) != nil
                    Button {
                        problem = nil
                        withAnimation(motion) { pick(folder) }
                        // A project started before goes straight to its prompt.
                        go(known ? .prompt : .agent)
                    } label: {
                        HStack(spacing: 7) {
                            Image(systemName: picked ? "checkmark.circle.fill" : "folder")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(picked ? agent.tint : .white.opacity(0.5))
                                .contentTransition(reduceMotion ? .identity : .symbolEffect(.replace.downUp))
                                .frame(width: 14)
                            Text(folder.name)
                                .font(.system(size: 13, weight: .medium))
                                .foregroundStyle(.white.opacity(picked ? 1 : 0.8))
                                .lineLimit(1)
                            Text(folder.path)
                                .font(.system(size: 10))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                                .truncationMode(.head)
                            Spacer(minLength: 0)
                            if known {
                                // Its last choices are kept: one click to the prompt.
                                Image(systemName: "arrow.uturn.forward")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(.white.opacity(0.35))
                                    .help(text.rememberedProject)
                            }
                        }
                        .padding(.horizontal, 10)
                        .frame(height: 32)
                        .background {
                            if picked {
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .fill(.white.opacity(0.1))
                                    .matchedGeometryEffect(id: "folder", in: folderHighlight)
                            }
                        }
                        .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    }
                    .buttonStyle(NotchButtonStyle(cornerRadius: 8, lifts: false))
                    .accessibilityLabel(folder.path)
                    .accessibilityAddTraits(picked ? .isSelected : [])
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

    /// Picks a project and takes on what it last started with, or the last
    /// choices of all projects when it has none.
    private func pick(_ folder: ClaudeRecentFolder) {
        let saved = NewSessionChoiceStore.load(folder.path)
        chosen = folder
        agentChoice = saved?.agent ?? ""
        modelChoices = saved?.models ?? [:]
        accountChoice = saved?.account
        worktreeChoice = saved?.worktree
        inBackground = saved?.background ?? true
        safeOn = saved?.safe ?? true
        safeOff = Set(saved?.safeOff ?? [])
        safeCustom = saved?.safeCustom ?? []
        worktreeName = ""
        checkRepository()
        let path = folder.path
        DispatchQueue.global(qos: .userInitiated).async {
            let found = SafeCommands.suggested(for: path)
            DispatchQueue.main.async { if chosen?.path == path { suggested = found } }
        }
    }

    private func checkRepository() {
        guard let path = chosen?.path else { repository = nil; return }
        repository = nil
        service.repositoryState(of: path) { state in
            if chosen?.path == path { repository = state }
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
        withAnimation(motion) { pick(folder) }
        problem = nil
        go(NewSessionChoiceStore.load(path) != nil ? .prompt : .agent)
    }

    // MARK: 2 · Agent

    private var agentPage: some View {
        VStack(spacing: 14) {
            FlowLayoutCentered(spacing: 8) {
                ForEach(agents, id: \.self) { provider in agentTile(provider) }
            }
            HStack(spacing: 8) {
                if picksAccount { accountMenu.transition(.opacity.combined(with: .offset(y: 4))) }
                if !models.isEmpty { modelMenu.transition(.opacity.combined(with: .offset(y: 4))) }
            }
        }
    }

    private func agentTile(_ provider: ClaudeSessionProvider) -> some View {
        let selected = provider == agent
        return Button {
            guard provider != agent else { return }
            problem = nil
            withAnimation(motion) { agentChoice = provider.rawValue }
        } label: {
            VStack(spacing: 7) {
                NotchSessionAgentGlyph(provider: provider, size: provider == .claude ? 26 : 23, working: false)
                    .frame(width: 32, height: 32)
                    .saturation(selected ? 1 : 0)
                    .opacity(selected ? 1 : 0.6)
                    .scaleEffect(selected && !reduceMotion ? 1.08 : 1)
                Text(provider.shortName)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.white.opacity(selected ? 1 : 0.6))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            .frame(width: 92, height: 80)
            .background {
                RoundedRectangle(cornerRadius: 14, style: .continuous).fill(.white.opacity(0.05))
                if selected {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(provider.tint.opacity(0.22))
                        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .strokeBorder(provider.tint.opacity(0.5), lineWidth: 1))
                        .matchedGeometryEffect(id: "agent", in: agentPill)
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 14, highlights: !selected))
        .help(provider.displayName)
        .accessibilityLabel(provider.displayName)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var accountMenu: some View {
        Menu {
            Picker(ClaudeAccountStrings.account, selection: Binding(
                get: { account?.id ?? "" },
                set: { id in withAnimation(motion) { accountChoice = id } })) {
                ForEach(shownAccounts) { option in Text(option.name).tag(option.id) }
            }
            .pickerStyle(.inline)
            .labelsHidden()
        } label: {
            menuLabel(symbol: "person.crop.circle", title: account?.name ?? ClaudeAccounts.defaultName, id: account?.id ?? "")
        }
        .menuStyle(.button)
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .menuIndicator(.hidden)
        .fixedSize()
        .help(account.map { ClaudeAccountStrings.startsWith($0.name) } ?? ClaudeAccountStrings.account)
        .accessibilityLabel(ClaudeAccountStrings.account)
        .accessibilityValue(account?.name ?? "")
    }

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
            menuLabel(symbol: "cpu", title: modelLabel, id: "\(agent.rawValue)-\(modelLabel)")
        }
        .menuStyle(.button)
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .menuIndicator(.hidden)
        .fixedSize()
        .animation(motion, value: modelLabel)
        .help(text.modelPicker)
        .accessibilityLabel("\(text.modelPicker): \(modelLabel)")
    }

    private func menuLabel(symbol: String, title: String, id: String) -> some View {
        HStack(spacing: 6) {
            Image(systemName: symbol)
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(.white.opacity(0.5))
            Text(title)
                .font(.system(size: 12.5, weight: .medium))
                .foregroundStyle(.white.opacity(0.9))
                .lineLimit(1)
                .id(id)
                .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: 5)),
                                        removal: .opacity.combined(with: .offset(y: -5))))
            Image(systemName: "chevron.up.chevron.down")
                .font(.system(size: 8.5, weight: .bold))
                .foregroundStyle(.white.opacity(0.45))
        }
        .padding(.horizontal, 12)
        .frame(height: 32)
        .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        .clipped()
    }

    private func loadModels() {
        service.newSessionModels(configHome: configHome) { claude, codex in
            claudeModels = claude
            codexModels = codex
        }
    }

    // MARK: 3 · Options

    private var optionsPage: some View {
        VStack(alignment: .leading, spacing: 6) {
            worktreeOption
            if offersSafeCommands { safeOption.transition(.opacity.combined(with: .offset(y: -4))) }
            runOption
        }
    }

    /// One option: its mark, its name and a control, with more below when on.
    private func option<Control: View, Detail: View>(
        _ symbol: String, _ title: String, on: Bool, help: String? = nil,
        @ViewBuilder control: () -> Control, @ViewBuilder detail: () -> Detail
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image(systemName: symbol)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(on ? agent.tint : .white.opacity(0.55))
                    .frame(width: 24, height: 24)
                    .background((on ? agent.tint.opacity(0.18) : .white.opacity(0.06)),
                                in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                    .symbolEffect(.bounce, options: .nonRepeating, value: reduceMotion ? false : on)
                Text(title)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.white.opacity(0.9))
                    .lineLimit(1)
                Spacer(minLength: 4)
                control()
            }
            .help(help ?? title)
            detail()
        }
        .padding(7)
        .background(.white.opacity(0.04), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func toggle(_ isOn: Binding<Bool>, label: String) -> some View {
        Toggle(label, isOn: Binding(get: { isOn.wrappedValue }, set: { value in withAnimation(motion) { isOn.wrappedValue = value } }))
            .toggleStyle(.switch)
            .controlSize(.mini)
            .labelsHidden()
            .tint(agent.tint)
            .accessibilityLabel(label)
    }

    private var worktreeOption: some View {
        option("arrow.triangle.branch", text.ownWorktree, on: usesWorktree,
               help: "\(text.ownWorktreeHelp) \(text.worktreeIgnoredFiles)") {
            toggle(Binding(get: { usesWorktree }, set: { worktreeChoice = $0 }), label: text.ownWorktree)
                .disabled(!isRepository)
        } detail: {
            if usesWorktree {
                TextField(worktreePlaceholder, text: $worktreeName)
                    .textFieldStyle(.plain)
                    .font(.system(size: 11.5, design: .monospaced))
                    .padding(.horizontal, 8)
                    .frame(height: 24)
                    .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                    .accessibilityLabel(text.worktreeNamePlaceholder)
                    .transition(.opacity.combined(with: .offset(y: -4)))
            }
            if let note = worktreeNote {
                Text(note)
                    .font(.system(size: 10.5))
                    .foregroundStyle(.white.opacity(0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .id(note)
                    .transition(.opacity)
            }
        }
    }

    private var safeOption: some View {
        option("checkmark.shield", text.safeCommands, on: safeOn, help: text.safeCommandsHelp) {
            HStack(spacing: 6) {
                if safeOn {
                    Text("\(safeList.count)")
                        .font(.system(size: 10.5, weight: .semibold, design: .rounded))
                        .monospacedDigit()
                        .foregroundStyle(.white.opacity(0.6))
                        .contentTransition(.numericText())
                        .accessibilityLabel(text.safeCount(safeList.count))
                }
                toggle($safeOn, label: text.safeCommands)
            }
        } detail: {
            if safeOn {
                FlowLayoutLite(spacing: 4) {
                    ForEach(shownSuggested) { entry in
                        safeChip(entry.command, symbol: entry.kind.symbol, on: !safeOff.contains(entry.command)) {
                            if safeOff.contains(entry.command) { safeOff.remove(entry.command) } else { safeOff.insert(entry.command) }
                        }
                    }
                    ForEach(safeCustom, id: \.self) { command in
                        safeChip(command, symbol: SafeCommands.Kind.custom.symbol, on: true, removable: true) {
                            safeCustom.removeAll { $0 == command }
                        }
                        .transition(.scale(scale: 0.6).combined(with: .opacity))
                    }
                    addChip
                }
                .animation(motion, value: safeCustom)
                .transition(.opacity.combined(with: .offset(y: -4)))
            }
        }
    }

    private func safeChip(_ command: String, symbol: String, on: Bool, removable: Bool = false,
                          action: @escaping () -> Void) -> some View {
        Button { withAnimation(motion) { action() } } label: {
            HStack(spacing: 4) {
                Image(systemName: removable ? "xmark" : symbol)
                    .font(.system(size: 8.5, weight: .bold))
                    .symbolEffect(.bounce, options: .nonRepeating, value: reduceMotion ? false : on)
                Text(command)
                    .font(.system(size: 10.5, weight: .medium, design: .monospaced))
                    .lineLimit(1)
                    .strikethrough(!on, color: .white.opacity(0.3))
            }
            .foregroundStyle(.white.opacity(on ? 0.92 : 0.38))
            .padding(.horizontal, 7)
            .frame(height: 22)
            .background(on ? agent.tint.opacity(0.18) : .white.opacity(0.04), in: Capsule(style: .continuous))
            .overlay(Capsule(style: .continuous).strokeBorder(on ? agent.tint.opacity(0.4) : .white.opacity(0.08), lineWidth: 1))
            .contentShape(Capsule(style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 11))
        .help(removable ? text.removeCommand : text.safeCommandsHelp)
        .accessibilityLabel(command)
        .accessibilityValue(on ? "1" : "0")
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    @ViewBuilder
    private var addChip: some View {
        if adding {
            TextField(text.addCommandPlaceholder, text: $newCommand)
                .textFieldStyle(.plain)
                .font(.system(size: 10.5, design: .monospaced))
                .focused($commandFocused)
                .frame(width: 130)
                .padding(.horizontal, 8)
                .frame(height: 22)
                .background(.white.opacity(0.08), in: Capsule(style: .continuous))
                .overlay(Capsule(style: .continuous)
                    .strokeBorder(commandRefused ? Color(red: 1.0, green: 0.55, blue: 0.5) : agent.tint.opacity(0.5), lineWidth: 1))
                .offset(x: commandRefused && !reduceMotion ? 3 : 0)
                .onSubmit(addCommand)
                .onAppear { DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { commandFocused = true } }
                .onChange(of: newCommand) { _, _ in commandRefused = false }
                .help(commandRefused ? text.commandRefused : text.addCommand)
                .accessibilityLabel(text.addCommand)
                .transition(.scale(scale: 0.8, anchor: .leading).combined(with: .opacity))
        } else {
            Button { withAnimation(motion) { adding = true } } label: {
                Image(systemName: "plus")
                    .font(.system(size: 9.5, weight: .bold))
                    .foregroundStyle(.white.opacity(0.7))
                    .frame(width: 26, height: 22)
                    .background(.white.opacity(0.06), in: Capsule(style: .continuous))
                    .contentShape(Capsule(style: .continuous))
            }
            .buttonStyle(NotchButtonStyle(cornerRadius: 11))
            .help(text.addCommand)
            .accessibilityLabel(text.addCommand)
        }
    }

    private func addCommand() {
        guard let command = SafeCommands.cleaned(newCommand) else {
            if newCommand.trimmingCharacters(in: .whitespaces).isEmpty { cancelAdding(); return }
            problem = text.commandRefused
            withAnimation(reduceMotion ? nil : .spring(response: 0.12, dampingFraction: 0.2)) { commandRefused = true }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { withAnimation(motion) { commandRefused = false } }
            return
        }
        problem = nil
        withAnimation(motion) {
            if suggested.contains(where: { $0.command == command }) {
                safeOff.remove(command)
            } else if !safeCustom.contains(command) {
                safeCustom.append(command)
            }
            newCommand = ""
            adding = false
        }
    }

    private func cancelAdding() {
        withAnimation(motion) {
            adding = false
            newCommand = ""
            commandRefused = false
        }
    }

    @ViewBuilder
    private var runOption: some View {
        if canRunInBackground {
            option(background ? "moon.zzz" : "terminal", background ? text.runInBackground : text.runInTerminal, on: true) {
                HStack(spacing: 2) {
                    mode("moon.zzz", text.runInBackground, selected: inBackground) { inBackground = true }
                    mode("terminal", text.runInTerminal, selected: !inBackground) { inBackground = false }
                }
                .padding(2)
                .background(.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            } detail: { EmptyView() }
        } else {
            option("terminal", text.runInTerminal, on: true) { EmptyView() } detail: {
                Text(agent.reportsThroughHooks ? text.opensInTerminal(agent.displayName) : text.backgroundOffHelp)
                    .font(.system(size: 10.5))
                    .foregroundStyle(.white.opacity(0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func mode(_ symbol: String, _ title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button { withAnimation(motion) { action() } } label: {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.white.opacity(selected ? 1 : 0.5))
                .frame(width: 34, height: 22)
                .background(.white.opacity(selected ? 0.16 : 0), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 6))
        .help(title)
        .accessibilityLabel(title)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    // MARK: 4 · Prompt

    private var promptPage: some View {
        VStack(spacing: 10) {
            promptMark
            TextField(background ? text.firstPromptRequired : text.firstPrompt, text: $prompt, axis: .vertical)
                .textFieldStyle(.plain)
                .font(.system(size: 12.5))
                .lineLimit(3...6)
                .focused($promptFocused)
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .frame(maxWidth: .infinity, minHeight: 58, alignment: .topLeading)
                .background(.white.opacity(promptFocused ? 0.1 : 0.07), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .strokeBorder(agent.tint.opacity(promptFocused ? 0.55 : 0), lineWidth: 1))
                .animation(.easeOut(duration: 0.18), value: promptFocused)
                .onSubmit(startOrAsk)
                .accessibilityLabel(background ? text.firstPromptRequired : text.firstPrompt)
            recipe
        }
        .onAppear { promptFocused = true }
    }

    /// The agent about to start, in a soft glow of its colour, with the
    /// project's name; it comes alive once there is something to send.
    private var promptMark: some View {
        let typing = !trimmedPrompt.isEmpty
        return VStack(spacing: 6) {
            ZStack {
                Circle()
                    .fill(RadialGradient(colors: [agent.tint.opacity(typing ? 0.38 : 0.24), agent.tint.opacity(0)],
                                         center: .center, startRadius: 2, endRadius: 34))
                    .frame(width: 68, height: 68)
                    .scaleEffect(typing && !reduceMotion ? 1.08 : 1)
                Circle()
                    .strokeBorder(agent.tint.opacity(0.35), lineWidth: 1)
                    .frame(width: 44, height: 44)
                NotchSessionAgentGlyph(provider: agent, size: agent == .claude ? 24 : 21, working: typing && !reduceMotion)
                    .frame(width: 28, height: 28)
            }
            .frame(height: 52)
        }
        .animation(reduceMotion ? nil : .spring(response: 0.5, dampingFraction: 0.7), value: typing)
        .accessibilityHidden(true)
    }

    /// What the session starts with, one chip per choice; a click goes to its step.
    private var recipe: some View {
        FlowLayoutCentered(spacing: 5) {
            recipeChip(.project, symbol: "folder", title: chosen?.name ?? text.chooseFolder)
            Button { go(.agent) } label: {
                HStack(spacing: 4) {
                    NotchSessionAgentGlyph(provider: agent, size: agent == .claude ? 9.5 : 8.5, working: false)
                        .frame(width: 11, height: 11)
                    Text(models.isEmpty ? agent.shortName : "\(agent.shortName) · \(modelLabel)")
                        .font(.system(size: 10.5, weight: .medium))
                        .lineLimit(1)
                }
                .recipeChipStyle()
            }
            .buttonStyle(NotchButtonStyle(cornerRadius: 11))
            .help(text.stepAgent)
            if usesWorktree { recipeChip(.options, symbol: "arrow.triangle.branch", title: text.worktreeMark) }
            if !safeList.isEmpty { recipeChip(.options, symbol: "checkmark.shield", title: "\(safeList.count)", help: text.safeCount(safeList.count)) }
            recipeChip(.options, symbol: background ? "moon.zzz" : "terminal",
                       title: background ? text.runInBackground : text.runInTerminal)
        }
        .animation(motion, value: usesWorktree)
        .animation(motion, value: safeList.count)
    }

    private func recipeChip(_ target: Step, symbol: String, title: String, help: String? = nil) -> some View {
        Button { go(target) } label: {
            HStack(spacing: 4) {
                Image(systemName: symbol).font(.system(size: 9, weight: .semibold))
                Text(title).font(.system(size: 10.5, weight: .medium)).lineLimit(1)
            }
            .recipeChipStyle()
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 11))
        .help(help ?? title)
        .accessibilityLabel(help ?? title)
    }

    // MARK: Footer

    private var footer: some View {
        HStack(spacing: 6) {
            if step != .project {
                arrow("chevron.left", label: text.previousStep) { go(Step(rawValue: step.rawValue - 1) ?? .project) }
                    .transition(.opacity.combined(with: .scale(scale: 0.7)))
            }
            if let message = problem {
                Text(message)
                    .font(.system(size: 10.5))
                    .foregroundStyle(Color(red: 1.0, green: 0.55, blue: 0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .id(message)
                    .transition(.opacity.combined(with: .offset(y: 3)))
            }
            Spacer(minLength: 4)
            if step != .prompt {
                arrow("chevron.right", label: text.nextStep) { go(Step(rawValue: step.rawValue + 1) ?? .prompt) }
                    .disabled(chosen == nil)
                    .transition(.opacity.combined(with: .scale(scale: 0.7)))
            }
            startButton
        }
        .animation(motion, value: step)
    }

    private func arrow(_ symbol: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(.white.opacity(0.75))
                .frame(width: 28, height: 28)
                .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .help(label)
        .accessibilityLabel(label)
    }

    private var startButton: some View {
        Button(action: startOrAsk) {
            ZStack {
                HStack(spacing: 5) {
                    Image(systemName: needsPrompt ? "text.bubble" : "play.fill")
                        .font(.system(size: 9.5, weight: .bold))
                        .contentTransition(reduceMotion ? .identity : .symbolEffect(.replace))
                    Text(text.startSession).font(.system(size: 12, weight: .semibold))
                }
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
            .padding(.horizontal, 13)
            .frame(height: 28)
            .background(agent.tint.opacity(canStart && !needsPrompt || starting ? 0.85 : 0.35),
                        in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .animation(motion, value: starting)
            .animation(.easeOut(duration: 0.25), value: needsPrompt)
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .disabled(!canStart)
        .help(needsPrompt ? text.backgroundNeedsPrompt : text.startSession)
        .accessibilityLabel(text.startSession)
    }

    /// Starts, or, while a background session still lacks its prompt, goes
    /// to the prompt and says so.
    private func startOrAsk() {
        guard canStart else { return }
        if needsPrompt {
            problem = text.backgroundNeedsPrompt
            go(.prompt)
            return
        }
        start()
    }

    private func start() {
        guard let chosen, canStart else { return }
        starting = true
        problem = nil
        let picked = model?.id
        let configHome = agent == .claude ? self.configHome : chosen.configHome
        let safe = safeList
        let choices = NewSessionChoices(
            agent: agent.rawValue, account: picksAccount ? account?.id : accountChoice,
            models: modelChoices.merging([agent.rawValue: modelKey.wrappedValue]) { _, new in new },
            worktree: wantsWorktree, background: inBackground, safe: safeOn,
            safeOff: safeOff.sorted(), safeCustom: safeCustom)
        let agent = agent
        service.startSession(in: chosen.path, configHome: configHome, prompt: prompt, background: background,
                             provider: agent, model: picked, worktree: usesWorktree,
                             worktreeName: worktreeName, safeCommands: safe) { failure in
            starting = false
            problem = failure
            guard failure == nil else { return }
            NewSessionChoiceStore.save(choices, for: chosen.path)
            // The last choices of all projects, for a project started for the first time.
            lastAgent = agent.rawValue
            if agent == .claude { lastClaudeModel = choices.models?[agent.rawValue] ?? lastClaudeModel }
            if agent == .codex { lastCodexModel = choices.models?[agent.rawValue] ?? lastCodexModel }
            if picksAccount, let path = account?.path { lastAccount = path }
            var off = Set(noWorktree.split(separator: ",").map(String.init))
            if wantsWorktree { off.remove(agent.rawValue) } else { off.insert(agent.rawValue) }
            noWorktree = off.sorted().joined(separator: ",")
            service.composingSession = false
        }
    }
}

private extension View {
    func recipeChipStyle() -> some View {
        self
            .foregroundStyle(.white.opacity(0.75))
            .padding(.horizontal, 7)
            .frame(height: 22)
            .background(.white.opacity(0.06), in: Capsule(style: .continuous))
            .contentShape(Capsule(style: .continuous))
    }
}

/// Wraps its pieces into rows like FlowLayoutLite, each row centered.
struct FlowLayoutCentered: Layout {
    var spacing: CGFloat = 6

    private func rows(_ width: CGFloat, _ subviews: Subviews) -> [[(index: Int, size: CGSize)]] {
        var rows: [[(index: Int, size: CGSize)]] = [[]]
        var x: CGFloat = 0
        for (index, subview) in subviews.enumerated() {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                rows.append([])
                x = 0
            }
            rows[rows.count - 1].append((index, size))
            x += size.width + spacing
        }
        return rows
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 480
        let rows = rows(width, subviews)
        let height = rows.reduce(0) { $0 + ($1.map(\.size.height).max() ?? 0) } + spacing * CGFloat(max(0, rows.count - 1))
        let widest = rows.map { row in row.reduce(0) { $0 + $1.size.width } + spacing * CGFloat(max(0, row.count - 1)) }.max() ?? 0
        return CGSize(width: proposal.width ?? widest, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in rows(bounds.width, subviews) {
            let rowWidth = row.reduce(0) { $0 + $1.size.width } + spacing * CGFloat(max(0, row.count - 1))
            let rowHeight = row.map(\.size.height).max() ?? 0
            var x = bounds.minX + max(0, (bounds.width - rowWidth) / 2)
            for item in row {
                subviews[item.index].place(at: CGPoint(x: x, y: y + (rowHeight - item.size.height) / 2), proposal: .unspecified)
                x += item.size.width + spacing
            }
            y += rowHeight + spacing
        }
    }
}
