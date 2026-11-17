// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// Settings > Sessions > Remote hosts (issue #189): the switch, the hosts by
/// their ssh config alias, how each stands, and setting a host up or taking
/// Openbar off it, each after the person confirms.
struct RemoteHostsSettings: View {
    @ObservedObject private var service = ClaudeSessionService.shared
    @AppStorage(DefaultsKey.notchSessionsRemoteHostsEnabled) private var enabled = false
    @State private var hosts: [RemoteHost] = RemoteHostPrefs.hosts()
    @State private var draft = ""
    @State private var suggestions: [String] = []
    @State private var invalid = false
    @State private var confirming: (alias: String, install: Bool)?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            SettingsRow(symbol: "network", title: RemoteHostStrings.enable) {
                Toggle(RemoteHostStrings.enable, isOn: $enabled).labelsHidden().toggleStyle(.switch)
            }
            Text(RemoteHostStrings.hint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, settingsRowTextInset)
            if enabled {
                ForEach(hosts) { host in
                    row(host)
                        .padding(.leading, settingsRowTextInset)
                }
                addRow
                    .padding(.leading, settingsRowTextInset)
            }
        }
        .onChange(of: enabled) { _, _ in service.syncWithPreferences() }
        .onAppear { suggestions = RemoteHostPrefs.configAliases() }
        .alert(confirmTitle, isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } })) {
            Button(RemoteHostStrings.cancel, role: .cancel) { confirming = nil }
            Button(confirming?.install == false ? RemoteHostStrings.removeHooks : RemoteHostStrings.setUp) {
                if let confirming { service.changeRemoteHooks(confirming.alias, install: confirming.install) }
                confirming = nil
            }
        } message: {
            Text(confirmMessage)
        }
    }

    private var confirmTitle: String {
        guard let confirming else { return "" }
        return String(format: confirming.install ? RemoteHostStrings.confirmSetUpTitle : RemoteHostStrings.confirmRemoveTitle,
                      confirming.alias)
    }

    private var confirmMessage: String {
        guard let confirming else { return "" }
        return String(format: confirming.install ? RemoteHostStrings.confirmSetUp : RemoteHostStrings.confirmRemove,
                      confirming.alias)
    }

    private func row(_ host: RemoteHost) -> some View {
        let state = service.remoteStates[host.alias]
        let busy = service.remoteBusy.contains(host.alias)
        return VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Circle()
                    .fill(color(state))
                    .frame(width: 7, height: 7)
                    .accessibilityHidden(true)
                // The alias keeps its whole name; the actions sit below it.
                Text(host.alias)
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .layoutPriority(1)
                Spacer(minLength: 8)
                Button {
                    hosts.removeAll { $0 == host }
                    save()
                } label: {
                    Image(systemName: "minus.circle")
                }
                .buttonStyle(.borderless)
                .help(RemoteHostStrings.delete)
                .accessibilityLabel(RemoteHostStrings.delete + " " + host.alias)
            }
            Group {
                Text(RemoteHostStrings.state(state))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let failure = service.remoteFailures[host.alias] {
                    Label(failure, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if busy {
                    ProgressView().controlSize(.small)
                } else if case .connected(let info)? = state, info.hooksInstalled == true || info.client != nil {
                    Button(info.hooksInstalled == true ? RemoteHostStrings.removeHooks : RemoteHostStrings.setUp) {
                        confirming = (host.alias, info.hooksInstalled != true)
                    }
                    .controlSize(.small)
                    .fixedSize()
                }
            }
            .padding(.leading, 15)
        }
    }

    private var addRow: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                TextField(RemoteHostStrings.aliasPlaceholder, text: $draft)
                    .textFieldStyle(.roundedBorder)
                    .controlSize(.small)
                    .onSubmit(add)
                let unused = suggestions.filter { alias in !hosts.contains { $0.alias == alias } }
                if !unused.isEmpty {
                    Menu {
                        ForEach(unused, id: \.self) { alias in Button(alias) { draft = alias; add() } }
                    } label: {
                        Image(systemName: "list.bullet")
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                    .help(RemoteHostStrings.aliasPlaceholder)
                }
                Button(RemoteHostStrings.add, action: add)
                    .controlSize(.small)
                    .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            if invalid {
                Text(RemoteHostStrings.invalidAlias)
                    .font(.caption)
                    .foregroundStyle(.orange)
            }
        }
    }

    private func add() {
        let alias = draft.trimmingCharacters(in: .whitespaces)
        guard RemoteHost.isValid(alias) else { invalid = !alias.isEmpty; return }
        invalid = false
        draft = ""
        guard !hosts.contains(where: { $0.alias == alias }) else { return }
        hosts.append(RemoteHost(alias: alias))
        save()
    }

    private func save() {
        RemoteHostPrefs.save(hosts)
        service.syncWithPreferences()
    }

    private func color(_ state: RemoteHostState?) -> Color {
        switch state {
        case .connected?: return .green
        case .disconnected?: return .orange
        case .connecting?, nil: return .secondary
        }
    }
}
