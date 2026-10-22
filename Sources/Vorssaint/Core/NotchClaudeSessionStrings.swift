// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

struct NotchClaudeSessionStrings {
    let title: String
    let summary: String
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
    let answerInTerminal: String
    let open: String
    let enlarge: String
    let showDetails: String
    let hideDetails: String
    let rename: String
    let renamePlaceholder: String
    let conversation: String
    let noMessages: String
    let showAllFormat: String
    let showLess: String
    let shrink: String
    let smallerText: String
    let largerText: String
    let sendShortcut: String
    let pasteImages: String
    let removeImage: String
    let send: String
    let replyPlaceholder: String
    let replyUnavailable: String
    let replyWhileWaiting: String
    let replyWhileDialog: String
    let tasksFormat: String
    let subagents: String
    let moreSubagentsFormat: String
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
    let sendWhileWaiting: String
    let bridgeTitle: String
    let bridgeHint: String
    let installBridge: String
    let updateBridge: String
    let settingsUnreadable: String
    let settingsBusy: String
    let settingsNoBackup: String
    let bridgeMissing: String
    let antigravityMissing: String
    let bridgePackageFailed: String
    let bridgeRefused: String
    let openFailed: String
    let background: String

    func phase(_ phase: ClaudeSessionPhase) -> String {
        switch phase {
        case .generating: return generating
        case .runningTool: return runningTool
        case .waitingPermission: return waitingPermission
        case .waitingInput: return waitingInput
        case .background: return background
        case .finished: return finished
        case .idle: return idle
        }
    }

    func moreSubagents(_ count: Int) -> String { String(format: moreSubagentsFormat, count) }
    func tasks(done: Int, total: Int) -> String { String(format: tasksFormat, done, total) }
    func showAll(_ count: Int) -> String { String(format: showAllFormat, count) }
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
        summary: "Running Claude Code sessions and what each one needs from you.",
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
        answerInTerminal: "Answer this one in the terminal.",
        open: "Open",
        enlarge: "Enlarge",
        showDetails: "Show details",
        hideDetails: "Hide details",
        rename: "Rename",
        renamePlaceholder: "Name this session",
        conversation: "Conversation",
        noMessages: "Nothing said yet.",
        showAllFormat: "Show all %d",
        showLess: "Show less",
        shrink: "Shrink",
        smallerText: "Smaller text",
        largerText: "Larger text",
        sendShortcut: "⌘↩ sends",
        pasteImages: "⌘V or drop to add images",
        removeImage: "Remove image",
        send: "Send",
        replyPlaceholder: "Reply to Claude…",
        replyUnavailable: "Replies work in Terminal and Antigravity",
        replyWhileWaiting: "Answer the prompt first",
        replyWhileDialog: "Close the dialog in the terminal first",
        tasksFormat: "%1$d of %2$d tasks",
        subagents: "Subagents",
        moreSubagentsFormat: "%d more",
        recentTools: "Recent",
        lastReply: "Last reply",
        previewsOff: "Turn on previews in Settings to see what Claude wrote.",
        permissionNotice: "Needs permission",
        inputNotice: "Waiting for you",
        finishedNotice: "Done",
        finishedAfterFormat: "Done after %@",
        liveActivity: "Show sessions beside the camera",
        autoExpand: "Open the island when a session needs you",
        autoExpandHint: "It won’t replace a page you already have open.",
        contentPreview: "Show previews",
        contentPreviewHint: "Shows titles, commands, file names and the conversation. They stay in memory and are cleared when you turn this off.",
        hooksTitle: "Claude Code hooks",
        hooksHint: "Hooks let the island show which tool runs and answer permission prompts. They’re added to ~/.claude/settings.json next to your own, with a backup first. Without the app running, Claude Code asks in the terminal as usual.",
        hooksInstalled: "Installed",
        hooksMissing: "Not installed",
        installHooks: "Install Hooks",
        removeHooks: "Remove Hooks",
        hooksFailedFormat: "Couldn’t change the settings: %@",
        shortcutsHint: "⌘Y allows and ⌘N denies while the island has the keyboard.",
        sendFailed: "Couldn’t reach that terminal",
        sendWhileWaiting: "It’s waiting on a prompt now. Answer it first.",
        bridgeTitle: "Antigravity extension",
        bridgeHint: "Replies to sessions in Antigravity go through a small extension included with this app. It shows a terminal or types into it only when you ask.",
        installBridge: "Install Extension",
        updateBridge: "Update Extension",
        settingsUnreadable: "settings.json can’t be read, so it was left alone",
        settingsBusy: "Claude Code kept changing settings.json",
        settingsNoBackup: "Couldn’t keep a copy of settings.json, so it was left alone",
        bridgeMissing: "The extension is missing from the app",
        antigravityMissing: "Antigravity isn’t installed",
        bridgePackageFailed: "Couldn’t package the extension",
        bridgeRefused: "Antigravity refused the extension",
        openFailed: "Couldn’t find that window",
        background: "In background")
}
