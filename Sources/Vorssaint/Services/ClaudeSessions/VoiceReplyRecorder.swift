// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AVFoundation
import Combine
import Speech

/// Speaks a reply into a session's reply field. The microphone feeds
/// Apple's on-device recognizer and nothing else: when the language has no
/// recognizer on this Mac, it says so instead of asking a server. The words
/// fill the field as they come and go out by the same path as typed ones.
final class VoiceReplyRecorder: ObservableObject {
    static let shared = VoiceReplyRecorder()

    struct Problem: Equatable {
        let sessionID: String
        let kind: VoiceReplyProblem
    }

    @Published private(set) var machine = VoiceReplyMachine()
    /// The microphone's level, 0…1, ten times a second while recording.
    @Published private(set) var level: Float = 0
    @Published private(set) var problem: Problem?

    private let defaults = UserDefaults.standard
    private var engine: AVAudioEngine?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var ticker: Timer?
    private var finishTimeout: DispatchWorkItem?
    private var feeder: VoiceReplyFileFeeder?
    /// Bumped on every start and end, so a late callback from a recording
    /// that ended can't touch the next one.
    private var generation = 0
    private let meter = VoiceReplyMeter()
    private var watchers: Set<AnyCancellable> = []
    /// Whether the shortcut started the recording on screen, so its release
    /// stops that one only.
    private var heldByShortcut = false

    /// `--voice-reply-file <audio>` feeds a file in place of the microphone,
    /// through the same recognizer, for end-to-end runs without a voice.
    private let replayFile: URL? = {
        let arguments = ProcessInfo.processInfo.arguments
        guard let index = arguments.firstIndex(of: "--voice-reply-file"), index + 1 < arguments.count else { return nil }
        return URL(fileURLWithPath: arguments[index + 1])
    }()

    private init() {
        let service = ClaudeSessionService.shared
        service.$sessions
            .receive(on: DispatchQueue.main)
            .sink { [weak self] sessions in
                guard let self, let id = self.machine.sessionID else { return }
                if !sessions.contains(where: { $0.id == id }) { self.end(.sessionGone) }
            }
            .store(in: &watchers)
        // Another page or a closed island ends recording; the words stay.
        service.$focusedID
            .receive(on: DispatchQueue.main)
            .sink { [weak self] focused in
                guard let self, let id = self.machine.sessionID, focused != id else { return }
                self.end(.islandClosed)
            }
            .store(in: &watchers)
        NotchService.shared.$expanded
            .receive(on: DispatchQueue.main)
            .sink { [weak self] expanded in
                if !expanded { self?.end(.islandClosed) }
            }
            .store(in: &watchers)
    }

    static var isOn: Bool { UserDefaults.standard.bool(forKey: DefaultsKey.notchSessionsVoiceReplies) }

    func isRecording(_ session: ClaudeSession) -> Bool { machine.sessionID == session.id && machine.isActive }

    func problem(for session: ClaudeSession) -> VoiceReplyProblem? {
        problem?.sessionID == session.id ? problem?.kind : nil
    }

    func clearProblem() { problem = nil }

    /// The microphone button.
    func toggle(_ session: ClaudeSession) {
        if machine.sessionID == session.id, machine.isActive { end(.stopped) } else { start(session) }
    }

    /// Escape while recording; false when there was nothing to cancel.
    @discardableResult
    func cancel() -> Bool {
        guard machine.isActive else { return false }
        end(.cancelled)
        return true
    }

    /// The hold-to-talk shortcut went down: the page on screen, or the
    /// session last at work, which the island opens on.
    func shortcutPressed() {
        guard Self.isOn, !machine.isActive else { return }
        let service = ClaudeSessionService.shared
        let notch = NotchService.shared
        let showing = notch.expanded && notch.selected == .sessions ? service.focusedID : nil
        guard let session = VoiceReplySupport.shortcutTarget(sessions: service.sessions, showing: showing) else { return }
        if showing != session.id {
            service.focus(session.id)
            notch.open(.sessions)
        }
        heldByShortcut = true
        start(session)
    }

    func shortcutReleased() {
        guard heldByShortcut else { return }
        heldByShortcut = false
        end(.stopped)
    }

    func start(_ session: ClaudeSession) {
        guard Self.isOn, VoiceReplySupport.canSpeak(to: session) else { return }
        let draft = ClaudeSessionService.shared.drafts[session.id] ?? ""
        guard machine.start(session: session.id, draft: draft) else { return }
        machine.sendOnStop = defaults.bool(forKey: DefaultsKey.notchSessionsVoiceSendOnStop)
        problem = nil
        generation += 1
        let token = generation
        authorize { [weak self] failure in
            guard let self, self.generation == token else { return }
            if let failure { self.fail(failure, session: session.id) } else { self.begin(session: session.id, token: token) }
        }
    }

    func end(_ ending: VoiceReplyEnding) {
        guard machine.isActive else { return }
        if ending != .stopped { heldByShortcut = false }
        let id = machine.sessionID
        let effects = machine.end(ending)
        apply(effects, session: id)
    }

    // MARK: Permissions

    /// Asks for the microphone and speech recognition the first time only,
    /// from the press itself, never at launch.
    private func authorize(_ done: @escaping (VoiceReplyProblem?) -> Void) {
        let speech = { (next: @escaping () -> Void) in
            switch SFSpeechRecognizer.authorizationStatus() {
            case .authorized: next()
            case .notDetermined:
                SFSpeechRecognizer.requestAuthorization { status in
                    DispatchQueue.main.async { status == .authorized ? next() : done(.speechDenied) }
                }
            default: done(.speechDenied)
            }
        }
        if replayFile != nil { speech { done(nil) }; return }
        guard AVCaptureDevice.default(for: .audio) != nil else { done(.noMicrophone); return }
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: speech { done(nil) }
        case .notDetermined:
            Permissions.shared.requestMicrophone { granted in
                granted ? speech { done(nil) } : done(.microphoneDenied)
            }
        default: done(.microphoneDenied)
        }
    }

    // MARK: Recording

    private func begin(session id: String, token: Int) {
        let identifier = VoiceReplySupport.localeIdentifier(
            saved: defaults.string(forKey: DefaultsKey.notchSessionsVoiceLanguage) ?? "", app: L10n.shared.language)
        let name = Locale.current.localizedString(forIdentifier: identifier) ?? identifier
        guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: identifier)),
              recognizer.supportsOnDeviceRecognition
        else { fail(.notOnDevice(name), session: id); return }

        let request = SFSpeechAudioBufferRecognitionRequest()
        // On this Mac or not at all: never Apple's servers.
        request.requiresOnDeviceRecognition = true
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        request.taskHint = .dictation
        self.request = request
        meter.reset()

        if let replayFile {
            let feeder = VoiceReplyFileFeeder(url: replayFile, request: request, meter: meter)
            guard feeder.start() else { fail(.recognizerFailed, session: id); return }
            self.feeder = feeder
        } else {
            let engine = AVAudioEngine()
            let input = engine.inputNode
            let format = input.outputFormat(forBus: 0)
            guard format.sampleRate > 0, format.channelCount > 0 else { fail(.noMicrophone, session: id); return }
            let meter = meter
            input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
                request.append(buffer)
                meter.record(buffer)
            }
            engine.prepare()
            do { try engine.start() } catch {
                input.removeTap(onBus: 0)
                fail(.noMicrophone, session: id)
                return
            }
            self.engine = engine
        }

        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            let text = result?.bestTranscription.formattedString
            let isFinal = result?.isFinal ?? false
            DispatchQueue.main.async {
                guard let self, self.generation == token else { return }
                self.recognized(text, isFinal: isFinal, failed: error != nil && result == nil)
            }
        }
        machine.began(at: ProcessInfo.processInfo.systemUptime)
        ticker = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            self?.tick()
        }
    }

    private func recognized(_ text: String?, isFinal: Bool, failed: Bool) {
        let id = machine.sessionID
        if case .finishing = machine.phase, isFinal || failed {
            apply(machine.finished(text), session: id)
        } else if let text {
            apply(machine.partial(text, at: ProcessInfo.processInfo.systemUptime), session: id)
        } else if failed, machine.isRecording {
            // The recognizer gave up while the microphone was still open.
            let effects = machine.failed()
            apply(effects, session: id)
            if let id { problem = Problem(sessionID: id, kind: .recognizerFailed) }
        }
    }

    private func tick() {
        let value = meter.take()
        level = value
        let id = machine.sessionID
        apply(machine.tick(level: value, at: ProcessInfo.processInfo.systemUptime), session: id)
    }

    private func fail(_ kind: VoiceReplyProblem, session id: String) {
        let effects = machine.failed()
        apply(effects, session: id)
        problem = Problem(sessionID: id, kind: kind)
    }

    private func apply(_ effects: VoiceReplyMachine.Effects, session id: String?) {
        let service = ClaudeSessionService.shared
        if let draft = effects.draft, let id, service.sessions.contains(where: { $0.id == id }) {
            service.drafts[id] = draft.isEmpty ? nil : draft
        }
        if effects.finishAudio {
            stopAudio()
            request?.endAudio()
            // The recognizer's last words, or the last partial if it says nothing.
            let token = generation
            let timeout = DispatchWorkItem { [weak self] in
                guard let self, self.generation == token else { return }
                self.apply(self.machine.finished(nil), session: id)
            }
            finishTimeout = timeout
            DispatchQueue.main.asyncAfter(deadline: .now() + 3, execute: timeout)
        }
        if effects.abort {
            stopAudio()
            task?.cancel()
        }
        if !machine.isActive { tearDown() }
        if let text = effects.send, let id, let session = service.sessions.first(where: { $0.id == id }) {
            service.send(text, to: session) { [weak self] failure in
                guard let failure else { return }
                self?.problem = Problem(sessionID: id, kind: .sendFailed(failure))
            }
        }
    }

    private func stopAudio() {
        ticker?.invalidate()
        ticker = nil
        feeder?.stop()
        feeder = nil
        if let engine {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }
        engine = nil
        level = 0
    }

    /// Drops everything a recording held; no audio is kept.
    private func tearDown() {
        stopAudio()
        finishTimeout?.cancel()
        finishTimeout = nil
        task?.cancel()
        task = nil
        request = nil
        generation += 1
        heldByShortcut = false
    }
}

/// A language speech is recognized in, and whether this Mac can do it on
/// the device (the only way voice replies use).
struct VoiceReplyLanguage: Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let onDevice: Bool

    static func all() -> [VoiceReplyLanguage] {
        SFSpeechRecognizer.supportedLocales().map { locale in
            let id = locale.identifier(.bcp47)
            return VoiceReplyLanguage(id: id, name: Locale.current.localizedString(forIdentifier: id) ?? id,
                                      onDevice: SFSpeechRecognizer(locale: locale)?.supportsOnDeviceRecognition == true)
        }
        .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }
}

/// The loudest RMS since the last look, written from the audio thread.
final class VoiceReplyMeter: @unchecked Sendable {
    private let lock = NSLock()
    private var peak: Float = 0

    func record(_ buffer: AVAudioPCMBuffer) {
        guard let channels = buffer.floatChannelData, buffer.frameLength > 0 else { return }
        let samples = channels[0]
        var sum: Float = 0
        for index in 0..<Int(buffer.frameLength) { sum += samples[index] * samples[index] }
        let level = VoiceReplyMachine.level(rms: (sum / Float(buffer.frameLength)).squareRoot())
        lock.lock(); peak = max(peak, level); lock.unlock()
    }

    func take() -> Float {
        lock.lock(); defer { lock.unlock() }
        let value = peak
        peak = 0
        return value
    }

    func reset() { _ = take() }
}

/// Plays an audio file into the recognizer at its own pace, then quiet, as a
/// microphone would hear it.
final class VoiceReplyFileFeeder: @unchecked Sendable {
    private let url: URL
    private let request: SFSpeechAudioBufferRecognitionRequest
    private let meter: VoiceReplyMeter
    private var timer: DispatchSourceTimer?

    init(url: URL, request: SFSpeechAudioBufferRecognitionRequest, meter: VoiceReplyMeter) {
        self.url = url
        self.request = request
        self.meter = meter
    }

    func start() -> Bool {
        guard let file = try? AVAudioFile(forReading: url) else { return false }
        let format = file.processingFormat
        let chunk = AVAudioFrameCount(format.sampleRate / 10)
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "vorssaint.voice-reply.file"))
        timer.schedule(deadline: .now(), repeating: .milliseconds(100))
        timer.setEventHandler { [request, meter] in
            guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: chunk) else { return }
            if (try? file.read(into: buffer, frameCount: chunk)) == nil || buffer.frameLength == 0 {
                buffer.frameLength = chunk // silence once the file is over
                if let channels = buffer.floatChannelData {
                    for channel in 0..<Int(format.channelCount) { channels[channel].update(repeating: 0, count: Int(chunk)) }
                }
            }
            request.append(buffer)
            meter.record(buffer)
        }
        timer.resume()
        self.timer = timer
        return true
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }
}
