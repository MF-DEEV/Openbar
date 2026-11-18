// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// The Hooks page in Settings (#236).
enum HooksPageStrings {
    static let title = "Hooks"
    static let intro = "Openbar adds a few small entries to each agent's config so the island hears its sessions. Here you can see each file it touched, check that the entries still work, and fix or remove them. A copy of the file is saved next to it before every change."
    static let repairTitle = "Put lost hooks back"
    static let repairHint = "When Openbar starts, or a config file changes, entries that went missing come back. Entries you edited yourself are left alone."
    static let reinstall = "Reinstall"
    static let install = "Install"
    static let remove = "Remove"
    static let showInFinder = "Show in Finder"
    static let addFolder = "Add Folder…"
    static let chooseFolder = "Choose a config folder"
    static let forget = "Take off this page"
    static let otherTools = "Other tools"
    static let otherToolsHint = "Any command-line tool that runs hooks the way Claude Code does. Openbar adds the same entries to its settings file and lists its sessions with a plain mark."
    static let addTool = "Add Tool…"
    static let toolName = "Name"
    static let toolFile = "Settings file"
    static let chooseFile = "Choose…"
    static let chooseToolFile = "Choose the tool's settings file, or the folder it keeps one in"
    static let add = "Add"
    static let cancel = "Cancel"
    static let removeEverything = "Remove Everything…"
    static let removeEverythingHint = "Takes Openbar out of every config file listed here. Files go back to how they were before Openbar where nothing else changed in them, and a copy of each stays next to it."
    static let removeEverythingConfirm = "Remove Openbar from every config?"
    static let removeEverythingMessage = "Sessions stop showing until you reinstall the hooks here. A copy of every file is kept next to it."
    static let removeEverythingDone = "Openbar is out of every config file."
    static let checking = "Checking…"
    static let notInstalled = "Not installed on this Mac"
    static let installedApp = "Installed"
    static let noTools = "No tools added."
    static let codexOwnFolder = "Heard through Codex's own server. Add another Codex home to watch it too."
    static let geminiFolderHint = "A folder Gemini CLI reads through GEMINI_CLI_HOME, or its .gemini folder."

    static func state(_ state: HookTargetState) -> String {
        switch state {
        case .installed: return "Installed"
        case .outdated: return "Outdated: an older Openbar wrote these"
        case .changedByHand: return "Changed by hand"
        case .missing: return "Missing"
        case .unreadable: return "Can't read this file"
        case .skipped(let reason): return "Skipped: " + skip(reason)
        }
    }

    static func skip(_ reason: HookSkip) -> String {
        switch reason {
        case .folderMissing: return "the config folder doesn't exist yet"
        case .agentNotInstalled: return "the agent isn't installed"
        case .needsApproval: return "the hooks need approval inside the agent"
        case .switchedOff: return "turned off in Settings › Sessions"
        case .accountHidden: return "this account is hidden"
        case .nothingNeeded: return "nothing to install here"
        case .removedByYou: return "you removed them"
        }
    }

    /// What the person does in the agent, when it asks to approve hooks.
    static func approvalNote(_ provider: ClaudeSessionProvider, folder: String) -> String? {
        guard provider == .codex else { return nil }
        return "Start Codex with CODEX_HOME=\(folder), open /hooks, review Openbar's hooks and trust them. Until then Codex skips them."
    }

    static let changedHint = "Reinstall writes Openbar's entries again and keeps your other changes."
    static let unreadableHint = "Fix the file by hand, or move it aside. Openbar won't write into a file it can't read."

    static func folderNotAdded(_ failure: AgentFolders.AddFailure) -> String {
        switch failure {
        case .notFolder: return "That isn't a folder."
        case .alreadyListed: return "That folder is already on the page."
        case .tooMany: return "That agent has as many folders as Openbar keeps."
        }
    }

    static func toolNotAdded(_ failure: CustomHookTools.AddFailure) -> String {
        switch failure {
        case .noName: return "Give the tool a name."
        case .notInFolder: return "Pick a settings file inside an existing folder."
        case .alreadyListed: return "That file is already on the page."
        case .tooMany: return "Openbar keeps up to 8 tools."
        case .claudeAccount: return "That file belongs to a Claude Code account, which is listed above."
        }
    }

    static let repairedTitle = "Openbar put its hooks back"
    static func repairedBody(_ names: [String]) -> String {
        "They had gone missing from " + names.joined(separator: ", ") + ". You can turn this off in Settings › Hooks."
    }

    // Settings › Sessions

    static let linkTitle = "Hooks"
    static func linkCaption(working: Int, attention: Int) -> String {
        let files = working == 1 ? "1 config works" : "\(working) configs work"
        guard attention > 0 else { return files }
        return files + (attention == 1 ? ", 1 needs a look" : ", \(attention) need a look")
    }
    static let open = "Open Hooks"
}
