// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The words of Claude Code accounts (issue #216). The Sessions section
/// speaks English in every language for now, so these do too.
enum ClaudeAccountStrings {
    static let title = "Claude accounts"
    static let hint = "Every folder Claude Code keeps its sign-in in is an account: ~/.claude, and any other one you start it with through CLAUDE_CONFIG_DIR. Name them so sessions and limits say which is which."
    static let add = "Add Folder…"
    static let chooseFolder = "Choose a Claude Code config folder"
    static let notAdded = "That folder is already listed or isn’t a folder."
    static let namePlaceholder = "Name"
    static let hide = "Hide"
    static let show = "Show"
    static let remove = "Remove"
    static let hiddenNote = "Hidden: no hooks, limits or place in New session"
    static let installHere = "Install here too"
    static let update = "Update"
    static let account = "Account"
    static let unreadable = "Settings aren’t valid JSON"
    static let needsClaude = "Needs Claude Code"
    static let signIn = "Sign in with Claude Code in this account"
    static let updateClaude = "Update Claude Code to read limits"
    static let unreachable = "Couldn’t reach the account"
    static let refreshHint = "Limits show once Claude Code answers"
    static let check = "Check limits"

    static func hooksState(_ state: ClaudeAccountHooks?) -> String {
        switch state {
        case .installed?: return "Hooks installed"
        case .outdated?: return "Hooks out of date"
        case .missing?: return "No hooks yet"
        case .unreadable?: return unreadable
        case nil: return ""
        }
    }

    /// Under "Claude Code hooks" once there is more than one account.
    static func installedIn(_ count: Int, of total: Int) -> String {
        count == total ? "Installed in all \(total) accounts" : "Installed in \(count) of \(total) accounts"
    }

    static func startsWith(_ name: String) -> String { "Starts with the \(name) account" }
    static func sessionAccount(_ name: String) -> String { "Claude account " + name }
}
