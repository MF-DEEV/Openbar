// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

struct NotchClaudeSessionStrings {
    let title: String
    let hubDescription: String
    let settingsDescription: String
    let emptyTitle: String
    let emptyDetail: String
    let generating: String
    let runningTool: String
    let waitingPermission: String
    let waitingInput: String
    let finished: String
    let idle: String
    let allow: String
    let alwaysAllow: String
    let deny: String
    let open: String
    let send: String
    let replyPlaceholder: String
    let replyUnavailable: String
    let replyWhileWaiting: String
    let tasksFormat: String
    let subagents: String
    let recentTools: String
    let lastReply: String
    let previewsOff: String
    let permissionNotice: String
    let inputNotice: String
    let finishedNotice: String
    let finishedAfterFormat: String
    let liveActivity: String
    let autoExpand: String
    let autoExpandHint: String
    let contentPreview: String
    let contentPreviewHint: String
    let hooksTitle: String
    let hooksHint: String
    let hooksInstalled: String
    let hooksMissing: String
    let installHooks: String
    let removeHooks: String
    let hooksFailedFormat: String
    let shortcutsHint: String
    let sendFailed: String
    let openFailed: String

    func phase(_ phase: ClaudeSessionPhase) -> String {
        switch phase {
        case .generating: return generating
        case .runningTool: return runningTool
        case .waitingPermission: return waitingPermission
        case .waitingInput: return waitingInput
        case .finished: return finished
        case .idle: return idle
        }
    }

    func tasks(done: Int, total: Int) -> String { String(format: tasksFormat, done, total) }
    func finishedAfter(_ duration: String) -> String { String(format: finishedAfterFormat, duration) }
    func hooksFailed(_ reason: String) -> String { String(format: hooksFailedFormat, reason) }
}

extension FeatureStrings {
    /// Claude Code is a product name and the section follows only it, so the
    /// section speaks English in every language for now.
    static func notchClaudeSessions(_ language: AppLanguage) -> NotchClaudeSessionStrings {
        switch language {
        case .enUS, .ptBR, .es, .sk, .de, .fr, .it, .ru, .tr, .ja, .ko, .uk, .zhHans, .zhTW, .zhHK: return .enUS
        }
    }
}

extension NotchClaudeSessionStrings {
    static let enUS = NotchClaudeSessionStrings(
        title: "Sessions",
        hubDescription: "See every Claude Code session running on this Mac in the Dynamic Island, what it’s doing and where, then answer permission prompts and reply without leaving what you’re on.",
        settingsDescription: "Reads the state Claude Code keeps for each running session on this Mac. Prompts and replies stay unread unless you turn on previews, and nothing leaves your Mac.",
        emptyTitle: "No sessions running",
        emptyDetail: "Start Claude Code in Terminal or Antigravity and it shows up here.",
        generating: "Working",
        runningTool: "Running",
        waitingPermission: "Needs permission",
        waitingInput: "Needs you",
        finished: "Done",
        idle: "Idle",
        allow: "Allow",
        alwaysAllow: "Always",
        deny: "Deny",
        open: "Open",
        send: "Send",
        replyPlaceholder: "Reply to Claude…",
        replyUnavailable: "Replies work in Terminal and Antigravity",
        replyWhileWaiting: "Answer the prompt first",
        tasksFormat: "%1$d of %2$d tasks",
        subagents: "Subagents",
        recentTools: "Recent",
        lastReply: "Last reply",
        previewsOff: "Turn on previews in Settings to see what Claude wrote.",
        permissionNotice: "Needs permission",
        inputNotice: "Waiting for you",
        finishedNotice: "Done",
        finishedAfterFormat: "Done after %@",
        liveActivity: "Show sessions beside the camera",
        autoExpand: "Open the island when a session needs you",
        autoExpandHint: "Never takes over another page you have open.",
        contentPreview: "Show previews",
        contentPreviewHint: "Shows titles, commands, file names and Claude’s last reply. They stay in memory and are cleared when you turn this off.",
        hooksTitle: "Claude Code hooks",
        hooksHint: "Hooks let the island show which tool runs and answer permission prompts. They’re added to ~/.claude/settings.json next to your own, with a backup first. Without the app running, Claude Code asks in the terminal as usual.",
        hooksInstalled: "Installed",
        hooksMissing: "Not installed",
        installHooks: "Install Hooks",
        removeHooks: "Remove Hooks",
        hooksFailedFormat: "Couldn’t change the settings: %@",
        shortcutsHint: "⌘Y allows and ⌘N denies while the island has the keyboard.",
        sendFailed: "Couldn’t reach that terminal",
        openFailed: "Couldn’t find that window")
}
