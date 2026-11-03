// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// What went wrong with a voice reply, said under the reply field with a way
/// to fix it where there is one.
enum VoiceReplyProblem: Equatable {
    case noMicrophone
    case microphoneDenied
    case speechDenied
    /// The language has no on-device recognizer on this Mac; the island
    /// never falls back to Apple's servers.
    case notOnDevice(String)
    case recognizerFailed
    case sendFailed(ClaudeReplyFailure)
}

/// Why recording ended, which decides what happens to the words.
enum VoiceReplyEnding: Equatable {
    /// A click on the button, or the shortcut let go.
    case stopped
    /// Quiet for a moment after speech.
    case silence
    /// The time limit, so a forgotten microphone does not stay open.
    case cap
    /// Escape: the field goes back to what it held.
    case cancelled
    /// The session ended: nothing is written or sent.
    case sessionGone
    /// The island closed or moved to another page: the words stay, unsent.
    case islandClosed

    /// Whether "Send when I stop talking" may send what was heard.
    var sends: Bool { self == .stopped || self == .silence }
    /// Whether the words heard stay in the field.
    var keepsWords: Bool { self != .cancelled && self != .sessionGone }
}

/// The recording state on its own, without the microphone or the
/// recognizer, so every way in and out is testable.
struct VoiceReplyMachine: Equatable {
    enum Phase: Equatable {
        case idle
        /// Asking for permissions or starting the microphone.
        case preparing
        case recording
        /// The microphone is off; the last words are on their way.
        case finishing(VoiceReplyEnding)
    }

    /// What the recorder has to do after an event.
    struct Effects: Equatable {
        /// The reply field's new text, when it changes.
        var draft: String?
        /// Turn the microphone off and let the recognizer finish.
        var finishAudio = false
        /// Drop the recognizer without waiting for its last words.
        var abort = false
        /// Send this text to the session.
        var send: String?
    }

    static let silenceAfterSpeech: TimeInterval = 1.5
    static let cap: TimeInterval = 60
    /// Levels are 0…1 (see `level(rms:)`): above `speech` is a voice, below
    /// `quiet` is a room.
    static let speechLevel: Float = 0.35
    static let quietLevel: Float = 0.2

    private(set) var phase: Phase = .idle
    private(set) var sessionID: String?
    /// The field before recording, which Escape gives back.
    private(set) var snapshot = ""
    private(set) var transcript = ""
    private(set) var heardSpeech = false
    private(set) var startedAt: TimeInterval = 0
    private(set) var quietSince: TimeInterval?
    var sendOnStop = false

    var isActive: Bool { phase != .idle }
    var isRecording: Bool { phase == .recording }

    /// The press of the button or the shortcut.
    mutating func start(session: String, draft: String) -> Bool {
        guard phase == .idle else { return false }
        phase = .preparing
        sessionID = session
        snapshot = draft
        transcript = ""
        heardSpeech = false
        quietSince = nil
        return true
    }

    /// The microphone is open.
    mutating func began(at now: TimeInterval) {
        guard phase == .preparing else { return }
        phase = .recording
        startedAt = now
    }

    /// Permissions or the recognizer said no before a word was heard.
    mutating func failed() -> Effects {
        let keep = phase == .recording || isFinishing ? Self.compose(snapshot, transcript) : nil
        reset()
        return Effects(draft: keep, abort: true)
    }

    /// The words so far, which replace the ones before.
    mutating func partial(_ text: String, at now: TimeInterval) -> Effects {
        guard phase == .recording || isFinishing else { return Effects() }
        if text != transcript, !text.isEmpty {
            heardSpeech = true
            quietSince = nil
        }
        transcript = text
        return Effects(draft: Self.compose(snapshot, transcript))
    }

    /// The microphone's level, 0…1, and the time; ends on silence or the cap.
    mutating func tick(level: Float, at now: TimeInterval) -> Effects {
        guard phase == .recording else { return Effects() }
        if now - startedAt >= Self.cap { return end(.cap) }
        if level >= Self.speechLevel {
            heardSpeech = true
            quietSince = nil
        } else if level < Self.quietLevel, heardSpeech {
            let since = quietSince ?? now
            quietSince = since
            if now - since >= Self.silenceAfterSpeech { return end(.silence) }
        }
        return Effects()
    }

    /// Ends recording. A stop or silence waits for the last words; Escape and
    /// a session gone end at once.
    mutating func end(_ ending: VoiceReplyEnding) -> Effects {
        switch phase {
        case .idle:
            return Effects()
        case .preparing:
            let restore = ending == .cancelled ? snapshot : nil
            reset()
            return Effects(draft: restore, abort: true)
        case .finishing(let current):
            // Escape or the session going away while the last words come.
            guard !ending.keepsWords, current.keepsWords else { return Effects() }
            let restore = ending == .cancelled ? snapshot : nil
            reset()
            return Effects(draft: restore, abort: true)
        case .recording:
            if !ending.keepsWords {
                let restore = ending == .cancelled ? snapshot : nil
                reset()
                return Effects(draft: restore, abort: true)
            }
            phase = .finishing(ending)
            return Effects(finishAudio: true)
        }
    }

    /// The recognizer's last words (or the last partial, when it has none).
    mutating func finished(_ text: String?) -> Effects {
        guard case .finishing(let ending) = phase else { return Effects() }
        if let text, !text.isEmpty { transcript = text }
        let words = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        let draft = Self.compose(snapshot, transcript)
        let send = sendOnStop && ending.sends && !words.isEmpty ? draft : nil
        reset()
        return Effects(draft: draft, send: send)
    }

    private var isFinishing: Bool {
        if case .finishing = phase { return true }
        return false
    }

    private mutating func reset() {
        phase = .idle
        sessionID = nil
        snapshot = ""
        transcript = ""
        heardSpeech = false
        quietSince = nil
    }

    /// What was in the field, then the words heard, with one space between.
    static func compose(_ before: String, _ words: String) -> String {
        let words = words.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !words.isEmpty else { return before }
        guard let last = before.last else { return words }
        return last.isWhitespace ? before + words : before + " " + words
    }

    /// A buffer's RMS on a scale where a room is near 0 and a voice near the
    /// microphone reaches past the middle: -60…-10 dBFS mapped to 0…1.
    static func level(rms: Float) -> Float {
        guard rms > 0 else { return 0 }
        let decibels = 20 * log10(rms)
        return min(1, max(0, (decibels + 60) / 50))
    }
}

enum VoiceReplySupport {
    /// The session a held shortcut speaks to: the page on screen, or else
    /// the session last at work that can take a reply.
    static func shortcutTarget(sessions: [ClaudeSession], showing: String?) -> ClaudeSession? {
        if let showing, let session = sessions.first(where: { $0.id == showing }) {
            return canSpeak(to: session) ? session : nil
        }
        return sessions.filter(canSpeak).max { activity($0) < activity($1) }
    }

    /// Whether the microphone shows beside a session's field: the option is
    /// on and replies reach the session at all.
    static func showsMicrophone(for session: ClaudeSession, enabled: Bool) -> Bool {
        enabled && NotchClaudeSessionsSupport.repliesReach(session) && session.background?.ended != true
    }

    /// The same rule as the reply field: replies have to reach the session,
    /// and it is not waiting on an answer elsewhere.
    static func canSpeak(to session: ClaudeSession) -> Bool {
        NotchClaudeSessionsSupport.repliesReach(session)
            && !session.phase.isWaiting && session.waitingFor == nil && session.background?.ended != true
    }

    private static func activity(_ session: ClaudeSession) -> Date {
        session.lastActive ?? session.phaseSince
    }

    /// The recognizer locale for a saved choice, or the app's language.
    static func localeIdentifier(saved: String, app: AppLanguage) -> String {
        if !saved.isEmpty { return saved }
        switch app {
        case .enUS: return "en-US"
        case .ptBR: return "pt-BR"
        case .tr: return "tr-TR"
        case .ru: return "ru-RU"
        case .es: return "es-ES"
        case .sk: return "sk-SK"
        case .de: return "de-DE"
        case .fr: return "fr-FR"
        case .it: return "it-IT"
        case .ja: return "ja-JP"
        case .ko: return "ko-KR"
        case .uk: return "uk-UA"
        case .zhHans: return "zh-CN"
        case .zhTW: return "zh-TW"
        case .zhHK: return "zh-HK"
        }
    }
}
