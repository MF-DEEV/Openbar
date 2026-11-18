// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import SwiftUI

/// The Hooks page (#236): for every agent, each config file Openbar wrote
/// into, whether its entries still work, and Reinstall and Remove, a copy of
/// the file kept first. More config folders and tools with Claude Code style
/// hooks can be added, lost hooks come back by themselves, and one button
/// takes Openbar out of every file.
struct HooksSettings: View {
    @ObservedObject private var service = ClaudeSessionService.shared
    @AppStorage(DefaultsKey.hooksAutoRepair) private var autoRepair = true
    @State private var addFailure: String?
    @State private var addingTool = false
    @State private var confirmingRemoveAll = false
    @State private var removedAll: Int?

    private static let order: [ClaudeSessionProvider] = [.claude, .codex] + ClaudeSessionProvider.hookAgents

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text(HooksPageStrings.title).font(.title2.bold())
                Text(HooksPageStrings.intro)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                SettingsCard {
                    SettingsRow(symbol: "arrow.triangle.2.circlepath", title: HooksPageStrings.repairTitle,
                                caption: HooksPageStrings.repairHint) {
                        Toggle(HooksPageStrings.repairTitle, isOn: $autoRepair)
                            .labelsHidden()
                            .toggleStyle(.switch)
                            .onChange(of: autoRepair) { _, on in
                                if on { service.checkHookTargets(repair: true); service.syncAgentHooks() }
                            }
                    }
                }
                if let targets = service.hookTargets {
                    ForEach(Self.order, id: \.self) { provider in
                        let rows = targets.filter { $0.provider == provider }
                        if !rows.isEmpty || AgentHookInstaller.foldersByHand.contains(provider) {
                            HookAgentCard(provider: provider, rows: rows, onAddFolder: { addFolder(provider) })
                        }
                    }
                    toolsCard(targets.filter { $0.provider == .custom })
                    removeAllCard(targets)
                } else {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text(HooksPageStrings.checking).foregroundStyle(.secondary)
                    }
                }
            }
            .frame(maxWidth: 760)
            .frame(maxWidth: .infinity)
            .padding(22)
        }
        .onAppear { service.checkHookTargets() }
        // A file changed while the page shows reads again, even with the
        // section's watcher off.
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(5))
                guard !Task.isCancelled else { return }
                service.checkHookTargets()
            }
        }
        .alert(HooksPageStrings.chooseFolder, isPresented: Binding(get: { addFailure != nil }, set: { if !$0 { addFailure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(addFailure ?? "")
        }
        .sheet(isPresented: $addingTool) { AddHookToolSheet() }
        .confirmationDialog(HooksPageStrings.removeEverythingConfirm, isPresented: $confirmingRemoveAll) {
            Button(HooksPageStrings.remove, role: .destructive) {
                service.removeAllHooks { removedAll = $0 }
            }
            Button(HooksPageStrings.cancel, role: .cancel) {}
        } message: {
            Text(HooksPageStrings.removeEverythingMessage)
        }
    }

    private func toolsCard(_ tools: [HookTarget]) -> some View {
        SettingsCard {
            SettingsRow(symbol: "terminal", title: HooksPageStrings.otherTools, caption: HooksPageStrings.otherToolsHint) {
                Button(HooksPageStrings.addTool) { addingTool = true }
                    .controlSize(.small)
                    .fixedSize()
            }
            if tools.isEmpty {
                Text(HooksPageStrings.noTools)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .padding(.leading, settingsRowTextInset)
            }
            ForEach(tools) { target in
                Divider()
                HookTargetRow(target: target)
            }
        }
    }

    private func removeAllCard(_ targets: [HookTarget]) -> some View {
        SettingsCard {
            SettingsRow(symbol: "trash", title: HooksPageStrings.removeEverything.replacingOccurrences(of: "…", with: ""),
                        caption: HooksPageStrings.removeEverythingHint) {
                Button(HooksPageStrings.removeEverything, role: .destructive) { confirmingRemoveAll = true }
                    .controlSize(.small)
                    .fixedSize()
                    .disabled(!targets.contains(where: \.canRemove) || !service.hookTargetsBusy.isEmpty)
            }
            if removedAll == 0 {
                Label(HooksPageStrings.removeEverythingDone, systemImage: "checkmark.circle.fill")
                    .font(.caption)
                    .foregroundStyle(.green)
                    .padding(.leading, settingsRowTextInset)
                    .transition(.opacity)
            }
        }
    }

    private func addFolder(_ provider: ClaudeSessionProvider) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.showsHiddenFiles = true
        panel.message = provider == .gemini ? HooksPageStrings.geminiFolderHint : HooksPageStrings.chooseFolder
        panel.directoryURL = FileManager.default.homeDirectoryForCurrentUser
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if provider == .claude {
            if !service.addAccount(url) { addFailure = ClaudeAccountStrings.notAdded }
            service.checkHookTargets()
            return
        }
        addFailure = service.addHookFolder(provider, picked: url)
    }
}

/// One agent: its mark and name, whether it is installed, and a row for each
/// config file.
private struct HookAgentCard: View {
    let provider: ClaudeSessionProvider
    let rows: [HookTarget]
    let onAddFolder: () -> Void

    private var present: Bool { rows.contains(where: \.present) }

    var body: some View {
        SettingsCard {
            HStack(spacing: 12) {
                NotchSessionAgentGlyph(provider: provider, size: 15, working: false)
                    .frame(width: 26, height: 26)
                VStack(alignment: .leading, spacing: 2) {
                    Text(provider.displayName)
                    Text(present ? HooksPageStrings.installedApp : HooksPageStrings.notInstalled)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 12)
                if provider == .claude || AgentHookInstaller.foldersByHand.contains(provider) {
                    Button(HooksPageStrings.addFolder, action: onAddFolder)
                        .controlSize(.small)
                        .fixedSize()
                }
            }
            ForEach(rows) { target in
                Divider()
                HookTargetRow(target: target)
            }
        }
    }
}

/// One config file: its state and why, where it is, and what can be done.
struct HookTargetRow: View {
    let target: HookTarget
    @ObservedObject private var service = ClaudeSessionService.shared
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var busy: Bool { service.hookTargetsBusy.contains(target.id) }

    private var shownPath: String {
        let path = (target.file ?? target.folder).path
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
    }

    private var tint: Color {
        switch target.state {
        case .installed: return .green
        case .unreadable: return .red
        case .skipped(let reason): return reason == .needsApproval ? .orange : .secondary
        default: return .orange
        }
    }

    /// Install for a file that never had the hooks, Reinstall otherwise.
    private var installTitle: String {
        target.derived == .missing ? HooksPageStrings.install : HooksPageStrings.reinstall
    }

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Circle()
                .fill(tint)
                .frame(width: 8, height: 8)
                .padding(.top, 5)
                .padding(.leading, 9)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    if target.provider == .claude || target.manual {
                        Text(target.name).font(.callout.weight(.medium))
                    }
                    Text(HooksPageStrings.state(target.state))
                        .font(.callout)
                        .foregroundStyle(target.state == .installed ? .primary : .secondary)
                        .contentTransition(.opacity)
                }
                Text(target.provider == .codex && target.file == nil ? HooksPageStrings.codexOwnFolder : shownPath)
                    .font(target.file == nil ? .caption : .caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(target.file == nil ? nil : 1)
                    .truncationMode(.middle)
                    .help((target.file ?? target.folder).path)
                    .textSelection(.enabled)
                if case .skipped(.needsApproval) = target.state,
                   let approval = HooksPageStrings.approvalNote(target.provider, folder: shownFolder) {
                    hint(approval, symbol: "hand.raised")
                } else if target.state == .changedByHand {
                    hint(HooksPageStrings.changedHint, symbol: "pencil")
                } else if target.state == .unreadable {
                    hint(HooksPageStrings.unreadableHint, symbol: "exclamationmark.triangle")
                }
                if let failure = service.hookTargetFailures[target.id] {
                    Label(failure, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .animation(reduceMotion ? nil : .easeOut(duration: 0.2), value: target.state)
            Spacer(minLength: 8)
            HStack(spacing: 6) {
                if busy { ProgressView().controlSize(.small) }
                Button { reveal() } label: { Image(systemName: "folder") }
                    .buttonStyle(.borderless)
                    .help(HooksPageStrings.showInFinder)
                    .accessibilityLabel(HooksPageStrings.showInFinder)
                    .disabled(!FileManager.default.fileExists(atPath: target.folder.path))
                if target.canReinstall {
                    Button(installTitle) { service.changeHookTarget(target, install: true) }
                        .controlSize(.small)
                        .fixedSize()
                        .disabled(busy)
                }
                if target.canRemove {
                    Button(HooksPageStrings.remove) { service.changeHookTarget(target, install: false) }
                        .controlSize(.small)
                        .fixedSize()
                        .disabled(busy)
                }
                if target.manual {
                    Button { service.forgetHookTarget(target) } label: { Image(systemName: "minus.circle") }
                        .buttonStyle(.borderless)
                        .help(HooksPageStrings.forget)
                        .accessibilityLabel(HooksPageStrings.forget + " " + target.name)
                        .disabled(busy)
                }
            }
        }
    }

    private var shownFolder: String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let path = target.folder.path
        return path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
    }

    private func hint(_ text: String, symbol: String) -> some View {
        Label(text, systemImage: symbol)
            .font(.caption)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }

    private func reveal() {
        if let file = target.file, FileManager.default.fileExists(atPath: file.path) {
            NSWorkspace.shared.activateFileViewerSelecting([file])
        } else {
            NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: target.folder.path)
        }
    }
}

/// Adding a tool with Claude Code style hooks: its name and settings file.
private struct AddHookToolSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var name = ""
    @State private var file: URL?
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(HooksPageStrings.addTool.replacingOccurrences(of: "…", with: "")).font(.headline)
            Text(HooksPageStrings.otherToolsHint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            TextField(HooksPageStrings.toolName, text: $name)
                .textFieldStyle(.roundedBorder)
            HStack(spacing: 8) {
                Text(file?.path ?? HooksPageStrings.toolFile)
                    .font(.caption.monospaced())
                    .foregroundStyle(file == nil ? .tertiary : .secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button(HooksPageStrings.chooseFile) { choose() }
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(.orange)
            }
            HStack {
                Spacer()
                Button(HooksPageStrings.cancel, role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(HooksPageStrings.add) { add() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || file == nil)
            }
        }
        .padding(20)
        .frame(width: 440)
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.showsHiddenFiles = true
        panel.message = HooksPageStrings.chooseToolFile
        panel.directoryURL = FileManager.default.homeDirectoryForCurrentUser
        guard panel.runModal() == .OK, let url = panel.url else { return }
        // A folder stands for the settings.json it keeps.
        file = ClaudeAccounts.isFolder(url.path) ? url.appending(path: "settings.json") : url
    }

    private func add() {
        guard let file else { return }
        if let problem = service.addCustomHookTool(name: name, file: file) {
            failure = problem
        } else {
            dismiss()
        }
    }
}
