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
    /// A session the plan's usage limit stopped, and when it goes on.
    let limited: String
    let resumesFormat: String
    /// The plan limits on the island, and their setting.
    let limitsSetting: String
    let limitsSettingHint: String
    let allow: String
    let alwaysAllow: String
    /// Always while it allows every request of a Codex turn.
    let alwaysOn: String
    let alwaysOnHelp: String
    let alwaysOffHelp: String
    let cancelAlways: String
    let cancelAlwaysHelp: String
    let deny: String
    /// A plan Claude asks to have approved, and its two answers.
    let plan: String
    let approvePlan: String
    let keepPlanning: String
    /// The modes a plan is approved into, as Claude Code's plan dialog
    /// offers them, each with what it does.
    let planAuto: String
    let planAutoHelp: String
    let planAcceptEdits: String
    let planAcceptEditsHelp: String
    let planBypass: String
    let planBypassHelp: String
    let planManual: String
    let planManualHelp: String
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
    let imageFormat: String
    let imageUnavailable: String
    let send: String
    let replyPlaceholder: String
    let replyPlaceholderCodex: String
    let replyUnavailable: String
    /// Replies can't reach the app showing this session, named.
    let replyUnavailableFormat: String
    let replyKittyRemote: String
    let openInFormat: String
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
    let waitingSound: String
    let waitingSoundHint: String
    let waitingSoundChoice: String
    let playSound: String
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
    /// The same, on a card that offers Always.
    let shortcutsHintAlways: String
    /// The same, on a plan's card, with the mode ⌘Y picks.
    let shortcutsHintPlanFormat: String
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
    let editorMissingFormat: String
    let bridgePackageFailed: String
    let bridgeRefusedFormat: String
    let bridgeNoEditors: String
    let openFailed: String
    /// A whole session running with no terminal, and what can be done with it.
    let backgroundMark: String
    let backgroundHelp: String
    let backgroundStopped: String
    let backgroundFailed: String
    let backgroundNeedsReply: String
    let logs: String
    let logsTitle: String
    let logsEmpty: String
    let logsGone: String
    let stopSession: String
    let stopConfirm: String
    let removeSession: String
    let removeConfirm: String
    let keepSession: String
    let replyEnded: String
    let commandFailed: String
    /// The page that starts a session.
    let newSession: String
    let chooseFolder: String
    let browseFolder: String
    let noRecentFolders: String
    let firstPrompt: String
    let firstPromptRequired: String
    let runInBackground: String
    let runInTerminal: String
    let backgroundOffHelp: String
    let startSession: String
    let folderNotTrusted: String
    /// A session's own git worktree.
    let ownWorktree: String
    let ownWorktreeHelp: String
    let worktreeNamePlaceholder: String
    let worktreeNotRepo: String
    let worktreeFromHead: String
    let worktreeIgnoredFiles: String
    let worktreeFailed: String
    let worktreeMark: String
    let openWorktree: String
    let copyBranch: String
    let branchCopied: String
    let removeWorktree: String
    let removeWorktreeConfirm: String
    let worktreeUnsaved: String
    let removeAnyway: String
    let agentPicker: String
    let modelPicker: String
    let defaultModel: String
    let backgroundNeedsPrompt: String
    let codexNotStarted: String
    let codexFailed: String
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
        case .limited: return limited
        case .finished: return finished
        case .idle: return idle
        }
    }

    func resumes(_ when: String) -> String { String(format: resumesFormat, when) }

    /// What names the agent of a session, where Claude would otherwise be
    /// named. Codex's wording is the pattern for the agents after it.
    func question(for provider: ClaudeSessionProvider) -> String { named(question, questionCodex, provider) }
    func questionHint(for provider: ClaudeSessionProvider) -> String { named(questionHint, questionHintCodex, provider) }
    func replyPlaceholder(for provider: ClaudeSessionProvider) -> String {
        named(replyPlaceholder, replyPlaceholderCodex, provider)
    }
    func previewsOff(for provider: ClaudeSessionProvider) -> String { named(previewsOff, previewsOffCodex, provider) }
    /// The agent's own name, which is the same in every language.
    func agent(_ provider: ClaudeSessionProvider) -> String {
        switch provider {
        case .claude: return "Claude"
        case .codex: return "Codex"
        default: return provider.displayName
        }
    }

    private func named(_ claude: String, _ codex: String, _ provider: ClaudeSessionProvider) -> String {
        switch provider {
        case .claude: return claude
        case .codex: return codex
        default: return codex.replacingOccurrences(of: "Codex", with: agent(provider))
        }
    }

    /// Where a request the island can't answer is answered.
    func answerIn(_ provider: ClaudeSessionProvider) -> String {
        provider.reportsThroughHooks ? String(format: answerInAgentFormat, provider.displayName) : answerInTerminal
    }

    /// The reply field of a session replies can't reach.
    func replyUnavailable(for session: ClaudeSession) -> String {
        if session.provider == .cursor { return String(format: replyInAgentFormat, session.provider.displayName) }
        return replyUnavailable(for: session.host)
    }

    func agentHint(_ provider: ClaudeSessionProvider) -> String {
        switch provider {
        case .gemini: return agentHintGemini
        case .copilot: return agentHintCopilot
        case .opencode: return agentHintOpenCode
        case .cursor: return agentHintCursor
        case .claude, .codex: return ""
        }
    }


    func moreSubagents(_ count: Int) -> String { String(format: moreSubagentsFormat, count) }
    func tasks(done: Int, total: Int) -> String { String(format: tasksFormat, done, total) }
    func showAll(_ count: Int) -> String { String(format: showAllFormat, count) }
    func leftRunning(_ count: Int) -> String { String(format: leftRunningFormat, count) }
    func finishedAfter(_ duration: String) -> String { String(format: finishedAfterFormat, duration) }
    func hooksFailed(_ reason: String) -> String { String(format: hooksFailedFormat, reason) }
    func editorMissing(_ name: String) -> String { String(format: editorMissingFormat, name) }
    func bridgeRefused(_ name: String) -> String { String(format: bridgeRefusedFormat, name) }
    func openIn(_ name: String) -> String { String(format: openInFormat, name) }

    /// What the reply field says when replies can't reach the session's
    /// terminal, naming that terminal when the island knows it.
    func replyUnavailable(for host: ClaudeSessionHost) -> String {
        if case .kitty = host { return replyKittyRemote }
        if case .remote = host { return RemoteHostStrings.replyNeedsTmux }
        guard let bundle = host.bundle else { return replyUnavailable }
        return String(format: replyUnavailableFormat, ClaudeTerminalApps.name(of: bundle))
    }
    func dressUp(_ name: String) -> String { String(format: dressUpFormat, name) }
    func image(_ number: Int) -> String { String(format: imageFormat, number) }

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
        emptyDetail: "Start Claude Code or Codex in a terminal or an editor and it shows up here.",
        generating: "Working",
        runningTool: "Running",
        waitingPermission: "Needs permission",
        waitingInput: "Needs you",
        finished: "Done",
        idle: "Idle",
        limited: "Limit reached",
        resumesFormat: "resumes %@",
        limitsSetting: "Show plan limits",
        limitsSettingHint: "Puts a Claude or Codex limit on the strip once it gets close, and says when a session stopped at its limit goes on. With the AI section off, the island keeps reading the limits for this.",
        allow: "Allow",
        alwaysAllow: "Always",
        alwaysOn: "Always: ON",
        alwaysOnHelp: "⌘T. Allows every request of this Codex task as it comes. Turns off when the task ends.",
        alwaysOffHelp: "Allowing every request of this Codex task. Click to turn it off.",
        cancelAlways: "Cancel",
        cancelAlwaysHelp: "Stop allowing this task’s requests by itself.",
        deny: "Deny",
        plan: "Plan to approve",
        approvePlan: "Approve",
        keepPlanning: "Keep planning",
        planAuto: "Auto mode",
        planAutoHelp: "Approve the plan and switch to auto mode, where a reviewer model screens what Claude does instead of asking you.",
        planAcceptEdits: "Accept edits",
        planAcceptEditsHelp: "Approve the plan and let Claude edit files without asking.",
        planBypass: "Bypass",
        planBypassHelp: "Approve the plan and bypass permissions, so Claude asks for nothing.",
        planManual: "Manual",
        planManualHelp: "Approve the plan and have Claude ask before each edit.",
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
        imageFormat: "Image %d",
        imageUnavailable: "Image unavailable",
        send: "Send",
        replyPlaceholder: "Reply to Claude…",
        replyPlaceholderCodex: "Reply to Codex…",
        replyUnavailable: "Replies can’t reach this session yet",
        replyUnavailableFormat: "Replies aren’t available in %@ yet",
        replyKittyRemote: "Turn on remote control in kitty.conf to reply",
        openInFormat: "Open in %@",
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
        waitingSound: "Sound when a session waits",
        waitingSoundHint: "Plays a short sound when a session asks for permission, asks a question or waits for you to approve a plan. It stays quiet when a turn simply ends, when the island already shows the session, and when the session's own terminal tab is in front.",
        waitingSoundChoice: "Sound",
        playSound: "Play",
        contentPreview: "Show previews",
        contentPreviewHint: "Shows titles, commands, file names and the conversation. They stay in memory and are cleared when you turn this off.",
        hooksTitle: "Claude Code hooks",
        hooksHint: "Hooks let the island show which tool runs, answer permission prompts and answer Claude’s questions. They’re added to ~/.claude/settings.json next to your own, with a backup first. Without the app running, Claude Code asks in the terminal as usual.",
        hooksInstalled: "Installed",
        hooksMissing: "Not installed",
        installHooks: "Install Hooks",
        removeHooks: "Remove Hooks",
        hooksFailedFormat: "Couldn’t change the settings: %@",
        shortcutsHint: "⌘Y allows and ⌘N denies, even while another app is in front.",
        shortcutsHintAlways: "⌘Y allows, ⌘T picks Always and ⌘N denies, even while another app is in front.",
        shortcutsHintPlanFormat: "⌘Y approves with %@ and ⌘N keeps planning, even while another app is in front.",
        sendFailed: "Couldn’t reach that terminal",
        sendWhileWaiting: "It’s waiting on a prompt now. Answer it first.",
        bridgeTitle: "Editor extension",
        bridgeHint: "Replies to sessions in the terminal of VS Code, Cursor, Windsurf or Antigravity go through a small extension included with this app. It shows a terminal or types into it only when you ask.",
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
        editorMissingFormat: "%@ isn’t installed",
        bridgePackageFailed: "Couldn’t package the extension",
        bridgeRefusedFormat: "%@ refused the extension",
        bridgeNoEditors: "No VS Code, Cursor, Windsurf or Antigravity on this Mac",
        openFailed: "Couldn’t find that window",
        backgroundMark: "Background",
        backgroundHelp: "Runs in the background, with no terminal. Open attaches one.",
        backgroundStopped: "Stopped",
        backgroundFailed: "Failed",
        backgroundNeedsReply: "Needs a reply",
        logs: "Logs",
        logsTitle: "Recent output",
        logsEmpty: "Nothing printed yet",
        logsGone: "A stopped session keeps no output. Open it to bring it back.",
        stopSession: "Stop",
        stopConfirm: "Stop this session? Its conversation is kept, and Open brings it back.",
        removeSession: "Remove",
        removeConfirm: "Remove this session for good? Claude Code deletes it, and its worktree when that’s safe.",
        keepSession: "Keep",
        replyEnded: "Open this session to continue it",
        commandFailed: "Claude Code couldn’t do that",
        newSession: "New session",
        chooseFolder: "Folder",
        browseFolder: "Browse…",
        noRecentFolders: "No recent projects yet",
        firstPrompt: "First prompt (optional)",
        firstPromptRequired: "First prompt",
        runInBackground: "Background",
        runInTerminal: "Terminal",
        backgroundOffHelp: "Agent View is off in Claude Code, so sessions can only start in a terminal.",
        startSession: "Start",
        folderNotTrusted: "Claude Code doesn’t trust this folder yet. Start it in a terminal once and say yes.",
        ownWorktree: "Own worktree",
        ownWorktreeHelp: "Runs the session in its own git worktree, so sessions on one project don’t change each other’s files.",
        worktreeNamePlaceholder: "Worktree name",
        worktreeNotRepo: "Not a git repo, so the session runs in the folder.",
        worktreeFromHead: "Uncommitted changes stay here. The worktree starts from the last commit.",
        worktreeIgnoredFiles: "Ignored files like .env aren’t copied into the worktree.",
        worktreeFailed: "Couldn’t make a worktree",
        worktreeMark: "Worktree",
        openWorktree: "Open folder",
        copyBranch: "Copy branch",
        branchCopied: "Copied",
        removeWorktree: "Remove worktree",
        removeWorktreeConfirm: "Remove this worktree? Its branch stays, with any commits on it.",
        worktreeUnsaved: "This worktree has unsaved changes. Remove anyway?",
        removeAnyway: "Remove anyway",
        agentPicker: "Agent",
        modelPicker: "Model",
        defaultModel: "Default",
        backgroundNeedsPrompt: "In the background, a session needs a first prompt.",
        codexNotStarted: "Codex’s background server didn’t start. Try Terminal.",
        codexFailed: "Codex couldn’t start the session",
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
                 .wonderTiara: "Wonder Woman tiara", .mouseEars: "Mouse ears", .bowEars: "Mouse ears with bow",
                 .sorcererHat: "Sorcerer’s hat"],
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
                 .flashEmblem: "Flash lightning", .lanternEmblem: "Green Lantern emblem",
                 .fairyWand: "Fairy wand", .magicLamp: "Magic lamp"])
}

extension NotchClaudeSessionStrings {
    // A file change's diff on the permission card.
    var diffNewFile: String { "New file" }
    var diffDeletedFile: String { "Deletes the file" }
    var diffReplacesFile: String { "Replaces the whole file" }
    var diffNewCell: String { "New cell" }
    var diffDeletedCell: String { "Deletes the cell" }
    var diffEveryMatch: String { "Every match" }
    func diffCell(_ id: String) -> String { "Cell \(id)" }
    func diffPlaces(_ count: Int) -> String { "\(count) places" }
    func diffMovesTo(_ path: String) -> String { "Moves to \(path)" }
    func diffMoreLines(_ count: Int) -> String { count == 1 ? "… 1 more line" : "… \(count) more lines" }
    func diffHiddenLines(_ count: Int) -> String { count == 1 ? "1 unchanged line" : "\(count) unchanged lines" }
    func diffShowAll(_ count: Int) -> String { count == 1 ? "Show the line" : "Show all \(count) lines" }
    var diffShowBelow: String { "Show \(ClaudeFileDiffs.expandStep) more lines below" }
    var diffShowAbove: String { "Show \(ClaudeFileDiffs.expandStep) more lines above" }
    func diffCounts(added: Int, removed: Int) -> String { "\(added) added, \(removed) removed" }
    func diffLineLabel(added: Bool, removed: Bool, number: Int?, text: String) -> String {
        let kind = added ? "Added" : removed ? "Removed" : "Unchanged"
        return [number.map { "\(kind), line \($0)" } ?? kind, text].joined(separator: ": ")
    }

    var answerInAgentFormat: String { "Answer this one in %@." }
    var replyInAgentFormat: String { "Reply in %@" }
    var agentHooksOutdated: String { "Needs an update" }
    var tryAgain: String { "Try Again" }
    /// An agent's state now that its hooks follow its switch by themselves.
    func agentAutoState(_ state: AgentHookInstaller.State, present: Bool, shown: Bool) -> String {
        if state == .unreadable { return agentHooksUnreadable }
        if !shown { return state == .missing ? "Off" : "Turning off…" }
        if state == .installed { return present ? "Connected" : "Connected, not found on this Mac" }
        if !present { return codexNotFound }
        return state == .outdated ? "Updating…" : "Connecting…"
    }
    func opensInTerminal(_ name: String) -> String { "\(name) opens in a terminal, with your first prompt." }
    var agentHooksUnreadable: String { "Its settings couldn’t be read" }
    var agentsTitle: String { "More agents" }
    var agentsHint: String {
        "Each agent installed on this Mac is connected by itself: the island adds its hooks or plugin next to your own, after a backup. Turning an agent off takes out only what the island added."
    }
    var agentHintGemini: String {
        "Hooks in ~/.gemini/settings.json, next to your own. Sessions show what runs and when Gemini CLI waits on you; you answer its permission prompts in the terminal, and replies are typed there."
    }
    var agentHintCopilot: String {
        "A hooks file of the island’s own in ~/.copilot/hooks. Sessions show what runs and when Copilot CLI waits on you; you answer its permission prompts in the terminal, and replies are typed there."
    }
    var agentHintOpenCode: String {
        "A plugin of the island’s own in ~/.config/opencode/plugins. It shows what each session does, and lets you allow or deny OpenCode’s permission requests and send replies from the island."
    }
    var agentHintCursor: String {
        "Hooks in ~/.cursor/hooks.json, next to your own, for Cursor’s agent and cursor-agent. Approvals stay in Cursor; Open takes you there."
    }
}

extension NotchClaudeSessionStrings {
    func planMode(_ mode: ClaudePlanMode) -> String {
        switch mode {
        case .auto: return planAuto
        case .acceptEdits: return planAcceptEdits
        case .bypass: return planBypass
        case .manual: return planManual
        }
    }

    func planModeHelp(_ mode: ClaudePlanMode) -> String {
        switch mode {
        case .auto: return planAutoHelp
        case .acceptEdits: return planAcceptEditsHelp
        case .bypass: return planBypassHelp
        case .manual: return planManualHelp
        }
    }
}
