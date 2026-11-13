// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

/// The Claude Code accounts on this Mac (issue #216): each config folder with
/// its name, whether its settings hold the hooks, and a way to hide it. A
/// folder Openbar did not find can be added by hand.
struct ClaudeAccountsSettings: View {
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var notAdded = false

    private var home: String { FileManager.default.homeDirectoryForCurrentUser.path }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            SettingsRow(symbol: "person.2", title: ClaudeAccountStrings.title) {
                Button(ClaudeAccountStrings.add) { addFolder() }
                    .controlSize(.small)
                    .fixedSize()
            }
            Text(ClaudeAccountStrings.hint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, settingsRowTextInset)
            ForEach(service.accounts) { account in
                ClaudeAccountSettingsRow(account: account, state: service.accountHooks[account.id],
                                         showsHooks: service.hooksInstalled != nil, home: home)
                    .padding(.leading, settingsRowTextInset)
                    .transition(.opacity)
            }
        }
        .animation(.easeOut(duration: 0.2), value: service.accounts)
        .alert(ClaudeAccountStrings.chooseFolder, isPresented: $notAdded) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(ClaudeAccountStrings.notAdded)
        }
    }

    private func addFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.showsHiddenFiles = true
        panel.message = ClaudeAccountStrings.chooseFolder
        panel.directoryURL = FileManager.default.homeDirectoryForCurrentUser
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if !service.addAccount(url) { notAdded = true }
    }
}

/// One account: its name to edit, its folder, its hooks and a way to hide it.
struct ClaudeAccountSettingsRow: View {
    let account: ClaudeAccount
    let state: ClaudeAccountHooks?
    let showsHooks: Bool
    let home: String
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var name = ""
    @FocusState private var editing: Bool

    /// `~/…` for a folder in the home.
    private var shortPath: String {
        account.path.hasPrefix(home + "/") ? "~" + account.path.dropFirst(home.count) : account.path
    }

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    TextField(ClaudeAccountStrings.namePlaceholder, text: $name)
                        .textFieldStyle(.roundedBorder)
                        .controlSize(.small)
                        .frame(width: 130)
                        .focused($editing)
                        .onSubmit(save)
                        .onChange(of: editing) { _, focused in if !focused { save() } }
                        .accessibilityLabel(ClaudeAccountStrings.account + " " + account.name)
                    Text(shortPath)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .help(account.path)
                }
                Text(account.hidden ? ClaudeAccountStrings.hiddenNote : ClaudeAccountStrings.hooksState(state))
                    .font(.caption)
                    .foregroundStyle(state == .unreadable && !account.hidden ? Color.orange : Color.secondary)
            }
            .opacity(account.hidden ? 0.55 : 1)
            Spacer(minLength: 8)
            if showsHooks, !account.hidden, state == .missing || state == .outdated {
                Button(state == .outdated ? ClaudeAccountStrings.update : ClaudeAccountStrings.installHere) {
                    service.installHooks(true, only: account)
                }
                .controlSize(.small)
                .fixedSize()
            }
            if account.manual {
                Button { service.removeAccount(account) } label: { Image(systemName: "minus.circle") }
                    .buttonStyle(.plain)
                    .help(ClaudeAccountStrings.remove)
                    .accessibilityLabel(ClaudeAccountStrings.remove + " " + account.name)
            } else {
                Button { service.setAccountHidden(account, !account.hidden) } label: {
                    Image(systemName: account.hidden ? "eye" : "eye.slash")
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(.plain)
                .help(account.hidden ? ClaudeAccountStrings.show : ClaudeAccountStrings.hide)
                .accessibilityLabel((account.hidden ? ClaudeAccountStrings.show : ClaudeAccountStrings.hide) + " " + account.name)
            }
        }
        .onAppear { name = account.name }
        .onChange(of: account.name) { _, value in if !editing { name = value } }
    }

    private func save() {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed != account.name else { return }
        // An empty name goes back to the one made from the folder.
        service.renameAccount(account, to: trimmed)
    }
}
