// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The words of containers (issue #330). The Sessions section speaks English
/// in every language for now, so these do too.
enum ContainerStrings {
    static let title = "Containers"
    static let enable = "Show sessions in containers"
    static let hint = "Sessions running in Docker containers and devcontainers show up here too, with the container’s name on them. Openbar runs docker exec; nothing is written into a container until you set it up. Off until you turn it on."
    static let running = "Running containers"
    static let refresh = "Look again"
    static let noneRunning = "No containers are running."
    static let allPicked = "Every running container is in the list above."
    static let noEngine = "No docker or podman command line was found."
    static let notRunning = "Not running. Openbar connects once it starts."
    static let relayEnded = "The connection into the container ended"
    static let unexpected = "The container answered in a way Openbar didn’t expect"
    static let add = "Add"
    static let delete = "Stop showing this container"
    static let devcontainer = "Dev container"
    static let hooksOn = "Connected · hooks set up"
    static let hooksOff = "Connected · not set up yet"
    static let hooksGone = "Its hooks are gone, as after a rebuild. Set it up again to see its permission requests."
    static let rebuilt = "The container was rebuilt and lost Openbar’s hooks. Set it up again to see its permission requests."
    static let setUp = "Set up"
    static let setUpAgain = "Set up again"
    static let removeHooks = "Remove from container"
    static let confirmSetUpTitle = "Set up Openbar in %@?"
    static let confirmSetUp = "Openbar adds its hooks to ~/.claude/settings.json inside %@ and keeps a copy of the file as it is now. Nothing else in the container changes, apart from the ~/.openbar folder. “Remove from container” takes them out again."
    static let confirmRemoveTitle = "Remove Openbar from %@?"
    static let confirmRemove = "Openbar takes its hooks out of ~/.claude/settings.json inside %@. If nothing else changed in the file, it goes back exactly as it was before setup."
    static let cancel = "Cancel"
    static let replyNeedsTmux = "Replies into a container reach sessions inside tmux only"

    static func cantStart(_ engine: String) -> String { engine + " could not start" }

    static func disconnected(_ reason: String) -> String { "Not connected: " + reason }

    static func state(_ state: RemoteHostState?, target: ContainerTarget) -> String {
        switch state {
        case nil, .connecting?: return RemoteHostStrings.connecting
        case .connected(let info)?:
            if info.hooksInstalled == true { return hooksOn }
            return gone(info, target: target) ?? hooksOff
        case .disconnected(let reason, _)?: return disconnected(reason)
        }
    }

    /// Why a container set up before has no hooks now: rebuilt, or taken out.
    static func gone(_ info: RemoteHostInfo, target: ContainerTarget) -> String? {
        guard info.hooksInstalled == false, let setUp = target.setUp else { return nil }
        return info.container.map { $0.id != setUp } == true ? rebuilt : hooksGone
    }
}
