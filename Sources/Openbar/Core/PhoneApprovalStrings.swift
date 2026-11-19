// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// Settings › Sessions › Phone approvals (Telegram), #214.
enum PhoneApprovalStrings {
    static let title = "Phone approvals (Telegram)"
    static let hint = "When you’re away from the computer, sessions that need you message your phone through a Telegram bot, and you can allow, deny or reply from there. Off until you link a bot."
    static let privacy = "Of all Openbar’s features, only this one sends data off your computer. Messages pass through Telegram’s servers and hold the agent, the project and the request: the command, file names and a short diff, a question, or a plan. With Don’t send code on, they hold only the agent, the project and the kind of request."
    static let step1 = "In Telegram, open BotFather and send /newbot."
    static let step2 = "Give the bot a name. BotFather replies with a token."
    static let step3 = "Paste the token here. Openbar keeps it in your keychain."
    static let openBotFather = "Open BotFather"
    static let tokenPlaceholder = "Bot token"
    static let useToken = "Use token"
    static let codeIntro = "Send this code to @%@ in Telegram, or scan it with your phone to open the chat with the code filled in."
    static let codeExpires = "The code works for 10 minutes."
    static let openInTelegram = "Open in Telegram"
    static let copyCode = "Copy code"
    static let cancel = "Cancel"
    static let linkPhone = "Link a phone"
    static let forgetBot = "Forget bot"
    static let botReady = "Bot @%@ is ready to link."
    static let linkedFormat = "Linked to %@ through @%@."
    static let sendTest = "Send a test message"
    static let unlink = "Unlink"
    static let unlinkConfirmTitle = "Unlink your phone?"
    static let unlinkConfirm = "Openbar stops sending and reading messages right away and removes the bot’s token from your keychain. The bot stays in Telegram; delete it in BotFather if you no longer need it."
    static let enable = "Send requests to my phone"
    static let awayAfter = "Send after"
    static let awayMinutes = "%d min without input"
    static let awayHint = "Messages also go out as soon as the screen locks."
    static let sendAlways = "Send even while I’m at the computer"
    static let quietHours = "Quiet hours"
    static let quietTo = "to"
    static let quietHint = "Messages still arrive, just without a sound."
    static let hideCode = "Don’t send code"
    static let hideCodeHint = "Messages carry only the agent, the project and the kind of request. Allow and Deny still work."
    static let tellEnds = "Tell me when a session finishes or stops on an error"
    static let keychainLabel = "Openbar Telegram bot"

    /// The session page's list of answers from the phone.
    static let answeredTitle = "From your phone"

    static func verdict(_ verdict: ClaudePhoneAnswer.Verdict) -> String {
        switch verdict {
        case .allowed: return "Allowed"
        case .always: return "Always allowed"
        case .denied: return "Denied"
        case .planApproved: return "Plan approved"
        case .keptPlanning: return "Kept planning"
        case .answered: return "Answered"
        case .skipped: return "Skipped"
        }
    }

    static let invalidToken = "That isn’t a working bot token. Copy the whole token BotFather sent."
    static let keychainFailed = "Openbar couldn’t save the token in your keychain."
    static let offline = "Can’t reach Telegram right now. Openbar keeps trying."
    static let conflict = "Something else is reading this bot’s messages: a webhook, or Openbar on another computer. Use a separate bot for each computer."
    static let revoked = "Telegram no longer accepts this bot’s token. Unlink and set the bot up again."
    static let codeExpired = "The code expired. Make a new one."
    static let tooManyTries = "Too many wrong codes were sent to the bot. Make a new one."
    static let pickTheRest = "Saved. Pick answers for the other questions, then tap Send."
}
