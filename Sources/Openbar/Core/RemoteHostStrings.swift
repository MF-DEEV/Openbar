// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The words of remote hosts (issue #189). The Sessions section speaks
/// English in every language for now, so these do too.
enum RemoteHostStrings {
    static let title = "Remote hosts"
    static let hint = "Sessions on machines you reach over SSH show up here too, with the host’s name on them. Openbar uses your own ssh config, keys and agent, and never asks for a password. Off until you turn it on."
    static let enable = "Show sessions on remote hosts"
    static let aliasPlaceholder = "Host alias from ~/.ssh/config"
    static let add = "Add"
    static let delete = "Remove host"
    static let setUp = "Set up"
    static let removeHooks = "Remove from host"
    static let connecting = "Connecting…"
    static let connected = "Connected"
    static let hooksOn = "Connected · hooks set up"
    static let hooksOff = "Connected · not set up yet"
    static let noClient = "Connected · needs nc, socat or python3 on the host for hooks"
    static let replyNeedsTmux = "Replies over SSH reach sessions inside tmux only"
    static let confirmSetUpTitle = "Set up Openbar on %@?"
    static let confirmSetUp = "Openbar adds its hooks to ~/.claude/settings.json on %@ and keeps a copy of the file as it is now. Nothing else on the host changes. “Remove from host” takes them out again."
    static let confirmRemoveTitle = "Remove Openbar from %@?"
    static let confirmRemove = "Openbar takes its hooks out of ~/.claude/settings.json on %@. If nothing else changed in the file, it goes back exactly as it was before setup."
    static let cancel = "Cancel"
    static let invalidAlias = "Use the alias of a Host in your ssh config."

    static func disconnected(_ reason: String) -> String { "Disconnected: " + reason }

    static func describe(_ failure: RemoteHostSetup.Failure) -> String {
        switch failure {
        case .unreachable(let reason): return reason.isEmpty ? "The host can’t be reached" : reason
        case .noClient: return "The host has none of nc, socat or python3 for the hooks to write with"
        case .unreadable: return "The host’s ~/.claude/settings.json isn’t JSON, so it was left alone"
        case .busy: return "Claude Code kept changing its settings; try again"
        }
    }

    static func state(_ state: RemoteHostState?) -> String {
        switch state {
        case nil, .connecting?: return connecting
        case .connected(let info)?:
            if info.hooksInstalled == true { return hooksOn }
            return info.client == nil ? noClient : hooksOff
        case .disconnected(let reason, _)?: return disconnected(reason)
        }
    }
}
