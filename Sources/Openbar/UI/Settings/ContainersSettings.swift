// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// Settings > Sessions > Containers (issue #330): the switch, the running
/// containers to pick from, how each picked one stands, and setting one up
/// or taking Openbar out of it, each after the person confirms. Nothing runs
/// in a container the person didn't pick.
struct ContainersSettings: View {
    @ObservedObject private var service = ClaudeSessionService.shared
    @AppStorage(DefaultsKey.notchSessionsContainersEnabled) private var enabled = false
    /// The picked containers as stored; the service writes a setup's mark here too.
    @AppStorage(DefaultsKey.notchSessionsContainers) private var stored = ""
    @State private var running: [ContainerInfo] = []
    @State private var listFailure: String?
    @State private var looking = false
    @State private var confirming: (target: ContainerTarget, install: Bool)?

    private var targets: [ContainerTarget] {
        _ = stored
        return ContainerPrefs.targets()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            SettingsRow(symbol: "shippingbox", title: ContainerStrings.enable) {
                Toggle(ContainerStrings.enable, isOn: $enabled).labelsHidden().toggleStyle(.switch)
            }
            Text(ContainerStrings.hint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, settingsRowTextInset)
            if enabled {
                ForEach(targets) { target in
                    row(target)
                        .padding(.leading, settingsRowTextInset)
                }
                pickList
                    .padding(.leading, settingsRowTextInset)
            }
        }
        .onChange(of: enabled) { _, on in
            service.syncWithPreferences()
            if on { look() }
        }
        .onAppear { if enabled { look() } }
        .alert(confirmTitle, isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } })) {
            Button(ContainerStrings.cancel, role: .cancel) { confirming = nil }
            Button(confirming?.install == false ? ContainerStrings.removeHooks : ContainerStrings.setUp) {
                if let confirming { service.changeRemoteHooks(confirming.target.alias, install: confirming.install) }
                confirming = nil
            }
        } message: {
            Text(confirmMessage)
        }
    }

    private var confirmTitle: String {
        guard let confirming else { return "" }
        return String(format: confirming.install ? ContainerStrings.confirmSetUpTitle : ContainerStrings.confirmRemoveTitle,
                      confirming.target.title)
    }

    private var confirmMessage: String {
        guard let confirming else { return "" }
        return String(format: confirming.install ? ContainerStrings.confirmSetUp : ContainerStrings.confirmRemove,
                      confirming.target.title)
    }

    private func row(_ target: ContainerTarget) -> some View {
        let state = service.remoteStates[target.alias]
        let busy = service.remoteBusy.contains(target.alias)
        return VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Circle()
                    .fill(color(state, target: target))
                    .frame(width: 7, height: 7)
                    .accessibilityHidden(true)
                Image(systemName: "shippingbox")
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(target.title)
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .layoutPriority(1)
                if target.folder != nil {
                    Text(ContainerStrings.devcontainer)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Button {
                    save(targets.filter { $0 != target })
                } label: {
                    Image(systemName: "minus.circle")
                }
                .buttonStyle(.borderless)
                .help(ContainerStrings.delete)
                .accessibilityLabel(ContainerStrings.delete + " " + target.title)
            }
            Group {
                if let folder = target.folder {
                    Text(folder)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.head)
                }
                Text(ContainerStrings.state(state, target: target))
                    .font(.caption)
                    .foregroundStyle(gone(state, target: target) ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
                    .fixedSize(horizontal: false, vertical: true)
                if let failure = service.remoteFailures[target.alias] {
                    Label(failure, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if busy {
                    ProgressView().controlSize(.small)
                } else if case .connected(let info)? = state {
                    let installed = info.hooksInstalled == true
                    Button(installed ? ContainerStrings.removeHooks
                           : gone(state, target: target) ? ContainerStrings.setUpAgain : ContainerStrings.setUp) {
                        confirming = (target, !installed)
                    }
                    .controlSize(.small)
                    .fixedSize()
                }
            }
            .padding(.leading, 15)
        }
    }

    /// The running containers not picked yet, each with Add.
    private var pickList: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(ContainerStrings.running)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 8)
                if looking {
                    ProgressView().controlSize(.small)
                } else {
                    Button(ContainerStrings.refresh, action: look)
                        .controlSize(.small)
                }
            }
            let picked = Set(targets.map(\.id))
            let unpicked = running.filter { !picked.contains($0.target.id) }
            if let listFailure {
                Text(listFailure)
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            } else if unpicked.isEmpty, !looking {
                Text(running.isEmpty ? ContainerStrings.noneRunning : ContainerStrings.allPicked)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            ForEach(unpicked) { container in
                HStack(spacing: 8) {
                    Image(systemName: "shippingbox")
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(container.title)
                            .font(.callout)
                            .lineLimit(1)
                        Text(container.isDevcontainer ? ContainerStrings.devcontainer + " · " + container.name
                             : container.image)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer(minLength: 8)
                    Button(ContainerStrings.add) { save(targets + [container.target]) }
                        .controlSize(.small)
                        .accessibilityLabel(ContainerStrings.add + " " + container.title)
                }
            }
        }
    }

    private func look() {
        guard !looking else { return }
        looking = true
        DispatchQueue.global(qos: .userInitiated).async {
            let result: Result<[ContainerInfo], ContainerListFailure> = ContainerEngine.find().map { $0.running() }
                ?? .failure(.noEngine)
            DispatchQueue.main.async {
                looking = false
                switch result {
                case .success(let list):
                    running = list
                    listFailure = nil
                case .failure(.noEngine):
                    running = []
                    listFailure = ContainerStrings.noEngine
                case .failure(.engine(let reason)):
                    running = []
                    listFailure = reason
                }
            }
        }
    }

    private func save(_ targets: [ContainerTarget]) {
        ContainerPrefs.save(targets)
        service.syncWithPreferences()
    }

    private func gone(_ state: RemoteHostState?, target: ContainerTarget) -> Bool {
        if case .connected(let info)? = state { return ContainerStrings.gone(info, target: target) != nil }
        return false
    }

    private func color(_ state: RemoteHostState?, target: ContainerTarget) -> Color {
        switch state {
        case .connected?: return gone(state, target: target) ? .orange : .green
        case .disconnected?: return .orange
        case .connecting?, nil: return .secondary
        }
    }
}
