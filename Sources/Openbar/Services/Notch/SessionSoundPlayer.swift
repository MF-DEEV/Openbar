// SPDX-License-Identifier: GPL-3.0-or-later
import AppKit

/// The existing NSSound path with per-instance volume and a retained player.
/// Only session sounds share this instance; timers, music and system volume do not.
enum SessionSoundPlayer {
    private static var playing: NSSound?
    static func stop() { playing?.stop(); playing = nil }

    @discardableResult static func play(_ choice: String, volume: Double) -> Bool {
        stop()
        guard choice != "none", volume.isFinite, volume > 0 else { return true }
        let sound: NSSound?
        if TextSnippetSupport.alertSoundNames.contains(choice) {
            sound = NSSound(contentsOfFile: TextSnippetSupport.systemSoundsPath + "/" + choice + ".aiff", byReference: true)
        } else {
            let name = choice == "openbar:soft" ? "soft" : choice == "openbar:alert" ? "alert" : "chime"
            // build.sh copies these exact WAVs; Rust embeds the same three files.
            var source = URL(fileURLWithPath: #filePath)
            for _ in 0..<5 { source.deleteLastPathComponent() }
            let development = source.appendingPathComponent("Resources/SessionSounds/\(name).wav")
            let bundled = Bundle.main.url(forResource: name, withExtension: "wav", subdirectory: "SessionSounds")
            sound = NSSound(contentsOf: bundled ?? development, byReference: true)
        }
        guard let sound else { return false }
        sound.volume = Float(min(100, volume) / 100)
        playing = sound
        return sound.play()
    }
}
