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
    let question: String
    let questionCodex: String
    let pickAny: String
    let ownAnswer: String
    let sendAnswer: String
    let skip: String
    let questionHint: String
    let questionHintCodex: String
    let open: String
    let enlarge: String
    let leftRunningFormat: String
    let showDetails: String
    let hideDetails: String
    let rename: String
    let renamePlaceholder: String
    let conversation: String
    let noMessages: String
    let answer: String
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
    let replyPlaceholderCodex: String
    let replyUnavailable: String
    let replyWhileWaiting: String
    let replyWhileDialog: String
    let tasksFormat: String
    let subagents: String
    let moreSubagentsFormat: String
    let recentTools: String
    let lastReply: String
    let previewsOff: String
    let previewsOffCodex: String
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
    let codexHint: String
    let codexConnected: String
    let codexNotRunning: String
    let codexNotFound: String
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
    let buddies: String
    let buddiesHint: String
    let wardrobe: String
    let dressUp: String
    let dressUpFormat: String
    let surpriseMe: String
    let ownOutfit: String
    let goToSession: String
    let shellTitle: String
    let hatTitle: String
    let eyewearTitle: String
    let extrasTitle: String
    let nothing: String
    let shells: [ClawdShell: String]
    let hats: [ClawdHat: String]
    let eyewears: [ClawdEyewear: String]
    let extras: [ClawdExtra: String]

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

    /// What names the agent of a session, where Claude would otherwise be named.
    func question(for provider: ClaudeSessionProvider) -> String { provider == .codex ? questionCodex : question }
    func questionHint(for provider: ClaudeSessionProvider) -> String { provider == .codex ? questionHintCodex : questionHint }
    func replyPlaceholder(for provider: ClaudeSessionProvider) -> String {
        provider == .codex ? replyPlaceholderCodex : replyPlaceholder
    }
    func previewsOff(for provider: ClaudeSessionProvider) -> String { provider == .codex ? previewsOffCodex : previewsOff }
    /// The agent's own name, which is the same in every language.
    func agent(_ provider: ClaudeSessionProvider) -> String { provider == .codex ? "Codex" : "Claude" }

    func moreSubagents(_ count: Int) -> String { String(format: moreSubagentsFormat, count) }
    func tasks(done: Int, total: Int) -> String { String(format: tasksFormat, done, total) }
    func showAll(_ count: Int) -> String { String(format: showAllFormat, count) }
    func leftRunning(_ count: Int) -> String { String(format: leftRunningFormat, count) }
    func finishedAfter(_ duration: String) -> String { String(format: finishedAfterFormat, duration) }
    func hooksFailed(_ reason: String) -> String { String(format: hooksFailedFormat, reason) }
    func dressUp(_ name: String) -> String { String(format: dressUpFormat, name) }

    func name(_ shell: ClawdShell) -> String { shells[shell] ?? shell.rawValue }
    func name(_ hat: ClawdHat) -> String { hat == .none ? nothing : hats[hat] ?? hat.rawValue }
    func name(_ eyewear: ClawdEyewear) -> String { eyewear == .none ? nothing : eyewears[eyewear] ?? eyewear.rawValue }
    func name(_ extra: ClawdExtra) -> String { extra == .none ? nothing : extras[extra] ?? extra.rawValue }
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
        summary: "Running Claude Code and Codex sessions, and what each one needs from you.",
        settingsDescription: "Reads the state Claude Code and Codex keep for each running session on this Mac. Prompts and replies stay unread unless you turn on previews, and nothing leaves your Mac.",
        emptyTitle: "No sessions running",
        emptyDetail: "Start Claude Code or Codex in Terminal or Antigravity and it shows up here.",
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
        question: "Claude asks",
        questionCodex: "Codex asks",
        pickAny: "Pick any that apply",
        ownAnswer: "Or write your own answer…",
        sendAnswer: "Answer",
        skip: "Skip",
        questionHint: "Your answer goes straight to Claude. Return sends what you wrote, and ⌘N skips the question.",
        questionHintCodex: "Your answer goes straight to Codex. Return sends what you wrote, and ⌘N skips the question.",
        open: "Open",
        enlarge: "Enlarge",
        leftRunningFormat: "%d left running",
        showDetails: "Show details",
        hideDetails: "Hide details",
        rename: "Rename",
        renamePlaceholder: "Name this session",
        conversation: "Conversation",
        noMessages: "Nothing said yet.",
        answer: "Summary",
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
        replyPlaceholderCodex: "Reply to Codex…",
        replyUnavailable: "Replies work in Terminal and Antigravity",
        replyWhileWaiting: "Answer the prompt first",
        replyWhileDialog: "Close the dialog in the terminal first",
        tasksFormat: "%1$d of %2$d tasks",
        subagents: "Subagents",
        moreSubagentsFormat: "%d more",
        recentTools: "Recent",
        lastReply: "Last reply",
        previewsOff: "Turn on previews in Settings to see what Claude wrote.",
        previewsOffCodex: "Turn on previews in Settings to see what Codex wrote.",
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
        hooksHint: "Hooks let the island show which tool runs, answer permission prompts and answer Claude’s questions. They’re added to ~/.claude/settings.json next to your own, with a backup first. Without the app running, Claude Code asks in the terminal as usual.",
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
        codexHint: "Nothing to install. The island talks to the app server Codex runs while it’s open, so approvals and replies go to Codex itself, not through the terminal.",
        codexConnected: "Connected",
        codexNotRunning: "Starts with Codex",
        codexNotFound: "Not found on this Mac",
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
        background: "In background",
        buddies: "A buddy for every session",
        buddiesHint: "Each session and subagent gets its own buddy, dressed at random: Clawd for Claude Code, a little robot for Codex. Click one on its session’s page to change the outfit.",
        wardrobe: "Wardrobe",
        dressUp: "Change outfit",
        dressUpFormat: "Change %@’s outfit",
        surpriseMe: "Surprise me",
        ownOutfit: "Back to the original outfit",
        goToSession: "Go to it",
        shellTitle: "Shell",
        hatTitle: "Hat",
        eyewearTitle: "Eyewear & masks",
        extrasTitle: "Extras",
        nothing: "None",
        shells: [.clay: "Clay", .coral: "Coral", .amber: "Amber", .moss: "Moss", .mint: "Mint", .teal: "Teal",
                 .sky: "Sky", .iris: "Iris", .orchid: "Orchid", .rose: "Rose", .midnight: "Midnight"],
        hats: [.beanie: "Beanie", .cap: "Cap", .beret: "Beret", .topHat: "Top hat", .crown: "Crown",
                 .party: "Party hat", .propeller: "Propeller cap", .chef: "Chef’s hat",
                 .doomHood: "Doctor Doom hood", .thorHelmet: "Thor helmet", .lokiHorns: "Loki horns",
                 .wonderTiara: "Wonder Woman tiara"],
        eyewears: [.glasses: "Glasses", .shades: "Sunglasses", .round: "Round glasses", .monocle: "Monocle",
                 .anaglyph: "3D glasses", .batmanMask: "Batman mask", .ironManMask: "Iron Man mask",
                 .spiderManMask: "Spider-Man mask", .doomMask: "Doctor Doom mask", .wolverineMask: "Wolverine mask",
                 .flashMask: "Flash mask", .captainMask: "Captain America mask", .deadpoolMask: "Deadpool mask",
                 .venomMask: "Venom mask", .blackPantherMask: "Black Panther mask", .daredevilMask: "Daredevil mask",
                 .starLordMask: "Star-Lord mask", .greenLanternMask: "Green Lantern mask", .robinMask: "Robin mask"],
        extras: [.bow: "Bow", .scarf: "Scarf", .bowTie: "Bow tie", .flower: "Flower", .headphones: "Headphones",
                 .coffee: "Coffee", .doomCape: "Doctor Doom cape", .strangeCape: "Doctor Strange cloak",
                 .thorCape: "Thor cape", .supermanCape: "Superman cape", .batmanCape: "Batman cape",
                 .captainShield: "Captain America shield", .mjolnir: "Thor / Mjolnir",
                 .aquamanTrident: "Aquaman trident", .lanternRing: "Green Lantern ring",
                 .wolverineClaws: "Wolverine claws", .batarang: "Batman batarang", .hawkeyeBow: "Hawkeye bow",
                 .strangeSpell: "Doctor Strange spell", .webShooter: "Spider-Man web",
                 .ironReactor: "Iron Man arc reactor", .supermanEmblem: "Superman emblem",
                 .flashEmblem: "Flash lightning", .lanternEmblem: "Green Lantern emblem"])
}
