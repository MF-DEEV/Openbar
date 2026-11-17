// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum VoiceReplyTests {
    static func run(_ suite: TestSuite) {
        suite.run("voice replies: start and stop") { startStop(suite) }
        suite.run("voice replies: cancel") { cancel(suite) }
        suite.run("voice replies: silence") { silence(suite) }
        suite.run("voice replies: cap") { cap(suite) }
        suite.run("voice replies: session gone and island closed") { gone(suite) }
        suite.run("voice replies: send when I stop talking") { sending(suite) }
        suite.run("voice replies: failures") { failures(suite) }
        suite.run("voice replies: text and level") { compose(suite) }
        suite.run("voice replies: shortcut target") { target(suite) }
        suite.run("voice replies: strings") { strings(suite) }
    }

    static func recording(draft: String = "", sendOnStop: Bool = false) -> VoiceReplyMachine {
        var machine = VoiceReplyMachine()
        machine.sendOnStop = sendOnStop
        _ = machine.start(session: "s", draft: draft)
        machine.began(at: 100)
        return machine
    }

    static func startStop(_ suite: TestSuite) {
        var machine = VoiceReplyMachine()
        suite.expect(machine.start(session: "s", draft: "fix"), "starts when idle")
        suite.expect(machine.phase == .preparing && machine.sessionID == "s", "prepares for the session")
        suite.expect(!machine.start(session: "t", draft: ""), "a second start is refused while one runs")
        machine.began(at: 10)
        suite.expect(machine.isRecording, "records once the microphone is open")

        let partial = machine.partial("the tests", at: 11)
        suite.expect(partial.draft == "fix the tests", "words follow what was typed, got \(String(describing: partial.draft))")
        let stop = machine.end(.stopped)
        suite.expect(stop.finishAudio && !stop.abort && stop.draft == nil, "stop waits for the last words")
        suite.expect(machine.phase == .finishing(.stopped), "finishing after a stop")
        suite.expect(machine.tick(level: 0, at: 200) == .init(), "no cap or silence while finishing")
        let done = machine.finished("the tests please")
        suite.expect(done.draft == "fix the tests please" && done.send == nil, "the last words stay, unsent")
        suite.expect(machine.phase == .idle && machine.sessionID == nil, "idle afterwards")
        suite.expect(machine.end(.stopped) == .init(), "stopping when idle does nothing")

        // Without final words, the last partial stays.
        var quiet = recording()
        _ = quiet.partial("hello", at: 101)
        _ = quiet.end(.stopped)
        suite.expect(quiet.finished(nil).draft == "hello", "the last partial when the recognizer says nothing")
    }

    static func cancel(_ suite: TestSuite) {
        var machine = recording(draft: "typed")
        _ = machine.partial("spoken words", at: 101)
        let cancelled = machine.end(.cancelled)
        suite.expect(cancelled.draft == "typed" && cancelled.abort, "Escape gives the field back")
        suite.expect(machine.phase == .idle, "idle after Escape")

        var preparing = VoiceReplyMachine()
        _ = preparing.start(session: "s", draft: "before")
        var prompted = VoiceReplyMachine()
        _ = prompted.start(session: "s", draft: "")
        suite.expect(prompted.end(.islandClosed) == .init() && prompted.phase == .preparing,
                     "the island closing behind a permission prompt keeps the press waiting")
        let early = preparing.end(.cancelled)
        suite.expect(early.draft == "before" && early.abort, "Escape while asking for permission")

        var finishing = recording(draft: "a")
        _ = finishing.partial("b", at: 101)
        _ = finishing.end(.stopped)
        let late = finishing.end(.cancelled)
        suite.expect(late.draft == "a" && late.abort && finishing.phase == .idle, "Escape while the last words come")
        suite.expect(finishing.finished("b c") == .init(), "late words after Escape are dropped")
    }

    static func silence(_ suite: TestSuite) {
        var machine = recording()
        // Quiet before any speech never ends it: the person may be thinking.
        for step in 0..<40 { _ = machine.tick(level: 0.05, at: 100 + Double(step) * 0.1) }
        suite.expect(machine.isRecording, "silence before speech keeps recording")
        _ = machine.tick(level: 0.6, at: 104.1)
        _ = machine.tick(level: 0.1, at: 104.2)
        suite.expect(machine.tick(level: 0.1, at: 105.6).finishAudio == false, "1.4 s of quiet is not yet silence")
        let ended = machine.tick(level: 0.1, at: 105.8)
        suite.expect(ended.finishAudio && machine.phase == .finishing(.silence), "1.5 s of quiet after speech ends it")

        // A word in between starts the count again.
        var talking = recording()
        _ = talking.tick(level: 0.6, at: 100.1)
        _ = talking.tick(level: 0.1, at: 100.2)
        _ = talking.partial("more", at: 101.2)
        _ = talking.tick(level: 0.1, at: 101.3)
        suite.expect(talking.tick(level: 0.1, at: 102.0).finishAudio == false, "new words reset the quiet")
        // Middle levels neither speak nor count as quiet.
        var middle = recording()
        _ = middle.tick(level: 0.6, at: 100.1)
        for step in 0..<30 { _ = middle.tick(level: 0.28, at: 100.2 + Double(step) * 0.1) }
        suite.expect(middle.isRecording, "a level between quiet and speech holds")
    }

    static func cap(_ suite: TestSuite) {
        var machine = recording(sendOnStop: true)
        _ = machine.partial("a long thought", at: 101)
        suite.expect(machine.tick(level: 0.6, at: 159.9).finishAudio == false, "under the cap")
        let capped = machine.tick(level: 0.6, at: 160)
        suite.expect(capped.finishAudio && machine.phase == .finishing(.cap), "the cap ends it at 60 s")
        let done = machine.finished(nil)
        suite.expect(done.draft == "a long thought" && done.send == nil, "the cap keeps the words but never sends")
    }

    static func gone(_ suite: TestSuite) {
        var machine = recording(draft: "x", sendOnStop: true)
        _ = machine.partial("words", at: 101)
        let gone = machine.end(.sessionGone)
        suite.expect(gone.abort && gone.draft == nil && gone.send == nil, "a session gone: nothing written or sent")
        suite.expect(machine.phase == .idle, "idle when the session ends")

        var closed = recording(draft: "x", sendOnStop: true)
        _ = closed.partial("words", at: 101)
        let closing = closed.end(.islandClosed)
        suite.expect(closing.finishAudio && !closing.abort, "the island closing waits for the last words")
        let done = closed.finished("words here")
        suite.expect(done.draft == "x words here" && done.send == nil, "the island closing keeps the words, unsent")

        var finishing = recording()
        _ = finishing.end(.stopped)
        let late = finishing.end(.sessionGone)
        suite.expect(late.abort && finishing.phase == .idle, "the session ending while the last words come")
    }

    static func sending(_ suite: TestSuite) {
        var stopped = recording(draft: "", sendOnStop: true)
        _ = stopped.partial("run the tests", at: 101)
        _ = stopped.end(.stopped)
        suite.expect(stopped.finished("Run the tests.").send == "Run the tests.", "sends after a stop")

        var silent = recording(draft: "Also", sendOnStop: true)
        _ = silent.tick(level: 0.6, at: 100.1)
        _ = silent.partial("check lint", at: 100.2)
        _ = silent.tick(level: 0.1, at: 100.3)
        _ = silent.tick(level: 0.1, at: 101.9)
        suite.expect(silent.finished(nil).send == "Also check lint", "sends after silence, with what was typed")

        var nothing = recording(sendOnStop: true)
        _ = nothing.end(.stopped)
        suite.expect(nothing.finished(nil).send == nil, "nothing heard: nothing sent")

        var off = recording(sendOnStop: false)
        _ = off.partial("hello", at: 101)
        _ = off.end(.stopped)
        suite.expect(off.finished(nil).send == nil, "the option off: never sends")
    }

    static func failures(_ suite: TestSuite) {
        var preparing = VoiceReplyMachine()
        _ = preparing.start(session: "s", draft: "typed")
        let denied = preparing.failed()
        suite.expect(denied.abort && denied.draft == nil && preparing.phase == .idle, "a refusal leaves the field alone")

        var midway = recording(draft: "typed")
        _ = midway.partial("half", at: 101)
        let broke = midway.failed()
        suite.expect(broke.draft == "typed half" && broke.abort, "a recognizer error keeps the words heard")
        suite.expect(midway.partial("late", at: 102) == .init(), "words after a failure are dropped")
    }

    static func compose(_ suite: TestSuite) {
        suite.expect(VoiceReplyMachine.compose("", "  hello ") == "hello", "trimmed words alone")
        suite.expect(VoiceReplyMachine.compose("fix", "it") == "fix it", "a space between")
        suite.expect(VoiceReplyMachine.compose("fix ", "it") == "fix it", "no doubled space")
        suite.expect(VoiceReplyMachine.compose("line\n", "next") == "line\nnext", "after a newline")
        suite.expect(VoiceReplyMachine.compose("kept", "") == "kept", "no words keeps the field")
        suite.expect(VoiceReplyMachine.level(rms: 0) == 0, "no sound")
        suite.expect(VoiceReplyMachine.level(rms: 0.001) == 0, "-60 dB is the floor")
        suite.expect(VoiceReplyMachine.level(rms: 1) == 1, "full scale is the top")
        suite.expectClose(Double(VoiceReplyMachine.level(rms: 0.01)), 0.4, "-40 dB", tol: 0.001)
        suite.expect(VoiceReplyMachine.level(rms: 0.01) >= VoiceReplyMachine.speechLevel, "-40 dB counts as a voice")
        suite.expect(VoiceReplyMachine.level(rms: 0.002) < VoiceReplyMachine.quietLevel, "-54 dB counts as a quiet room")
        suite.expect(VoiceReplyEnding.stopped.sends && VoiceReplyEnding.silence.sends, "stop and silence may send")
        suite.expect(!VoiceReplyEnding.cap.sends && !VoiceReplyEnding.islandClosed.sends, "cap and closing don't")
    }

    static func target(_ suite: TestSuite) {
        func session(_ id: String, _ phase: ClaudeSessionPhase = .idle, active: TimeInterval = 0,
                     host: ClaudeSessionHost = .terminal(tty: "/dev/ttys001")) -> ClaudeSession {
            var session = NotchSessionAgentTests.session(id, phase, provider: .claude)
            session.host = host
            session.lastActive = Date(timeIntervalSinceReferenceDate: active)
            return session
        }
        let old = session("old", active: 10)
        let recent = session("recent", active: 50)
        let waiting = session("waiting", .waitingPermission, active: 90)
        let unreachable = session("unknown", active: 99, host: .unknown)
        let all = [old, recent, waiting, unreachable]
        suite.expect(VoiceReplySupport.shortcutTarget(sessions: all, showing: "old")?.id == "old", "the page on screen")
        suite.expect(VoiceReplySupport.shortcutTarget(sessions: all, showing: nil)?.id == "recent",
                     "else the last at work that takes replies")
        suite.expect(VoiceReplySupport.shortcutTarget(sessions: all, showing: "waiting") == nil,
                     "a page waiting on an answer is not spoken to")
        suite.expect(VoiceReplySupport.shortcutTarget(sessions: [waiting, unreachable], showing: nil) == nil,
                     "nothing to speak to")
        suite.expect(VoiceReplySupport.shortcutTarget(sessions: all, showing: "gone")?.id == "recent",
                     "a page whose session ended falls back to the last at work")
        suite.expect(!VoiceReplySupport.showsMicrophone(for: unreachable, enabled: true), "no microphone where replies can't reach")
        suite.expect(VoiceReplySupport.showsMicrophone(for: recent, enabled: true), "a microphone where they can")
        suite.expect(!VoiceReplySupport.showsMicrophone(for: recent, enabled: false), "none with the option off")
    }

    static func strings(_ suite: TestSuite) {
        suite.expect(VoiceReplySupport.problem(domain: "kLSRErrorDomain", code: 201) == .dictationOff, "Dictation off")
        suite.expect(VoiceReplySupport.problem(domain: "kAFAssistantErrorDomain", code: 1700) == .dictationOff,
                     "Dictation off, as apps see it")
        suite.expect(VoiceReplySupport.problem(domain: "kAFAssistantErrorDomain", code: 1110) == .recognizerFailed,
                     "anything else is a failure to try again")
        suite.expect(VoiceReplySupport.localeIdentifier(saved: "", app: .de) == "de-DE", "follows the app")
        suite.expect(VoiceReplySupport.localeIdentifier(saved: "en-GB", app: .de) == "en-GB", "a chosen language wins")
        let english = NotchVoiceReplyStrings.forLanguage(.enUS)
        for language in AppLanguage.allCases {
            let strings = NotchVoiceReplyStrings.forLanguage(language)
            let all = Mirror(reflecting: strings).children.compactMap { $0.value as? String }
            suite.expect(all.count == 20 && all.allSatisfy { !$0.isEmpty }, "\(language): every string is there")
            if language != .enUS {
                suite.expect(strings.settingTitle != english.settingTitle, "\(language): translated")
            }
            suite.expect(strings.followApp("X").contains("X") && strings.notOnDevice("X").contains("X"),
                         "\(language): formats take the language")
        }
    }
}
