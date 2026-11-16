// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation

/// New copy uses the English fallback until a language supplies its translation.
struct SessionSoundStrings {
    let title = "Sounds"
    let caption = "Choose a sound for each event. Sounds stay quiet while you view that session. Preview uses this volume, even while notifications are muted."
    let none = "None"
    let chime = "Openbar chime"
    let soft = "Openbar drop"
    let alert = "Openbar alert"
    let volume = "Session sound volume"
    let unavailable = "unavailable; uses Openbar chime"
    let muted = "Session sounds muted until"
    let unmute = "Unmute session sounds"
    let hour = "Mute session sounds for 1 hour"
    let tomorrow = "Mute session sounds until tomorrow"
    let failed = "Could not play the sound. Check the audio output and try again."
    static func forLanguage(_ language: AppLanguage) -> Self { Self() }
    func event(_ kind: SessionSoundKind) -> String {
        switch kind {
        case .permission: "Permission request"
        case .question: "Question"
        case .finished: "Turn finished"
        case .error: "Stopped by an error"
        case .limit: "Stopped by a plan limit"
        case .started: "Session started"
        }
    }
    func preview(_ kind: SessionSoundKind) -> String { "Preview: " + event(kind) }
}
