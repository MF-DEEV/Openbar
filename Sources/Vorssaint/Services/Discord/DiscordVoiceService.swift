// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import ApplicationServices
import Combine
import CoreAudio

/// Discord's mute and deafen for the closed island, and whether the Mac's
/// own mute silences a microphone that is recording. Read-only throughout:
/// no audio stream is opened, Discord is never changed, no message is read,
/// and no login or token is needed. Live controls come first; the saved
/// voice state only seeds the start, and while Discord is hidden or
/// minimized its own shortcuts carry the state until its controls show again.
final class DiscordVoiceService: ObservableObject {
    static let shared = DiscordVoiceService()

    @Published private(set) var snapshot = VoiceChatSnapshot.empty

    private let queue = DispatchQueue(label: "com.vorssaint.utils.voicechat", qos: .utility)
    private let reader = DiscordVoiceReader()
    private let shortcuts = DiscordShortcutMonitor()
    private var timer: Timer?
    private var reading = false
    /// A read that finishes after the service stopped must not publish.
    private var generation = 0
    private var audioReadAt = Date.distantPast
    private var trusted = false
    private var trustCheckedAt = Date.distantPast
    /// When Discord was first read since it was last unknown.
    private var discordSince: Date?
    /// Long enough for Discord to build its accessibility tree and for its
    /// controls to replace the saved state they were seeded from.
    private static let settle: TimeInterval = 2.5

    private init() {}

    func syncWithPreferences() {
        if DiscordVoiceSupport.isEnabled() { start() } else { stop() }
    }

    private func start() {
        guard timer == nil else { return }
        let timer = Timer(timeInterval: 0.2, repeats: true) { [weak self] _ in self?.tick() }
        timer.tolerance = 0.05
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
        tick()
    }

    func stop() {
        guard timer != nil else { return }
        timer?.invalidate()
        timer = nil
        generation += 1
        reading = false
        trustCheckedAt = .distantPast
        discordSince = nil
        shortcuts.stop()
        let reader = reader
        queue.async { reader.reset() }
        if snapshot != .empty { snapshot = .empty }
    }

    private func tick() {
        guard !reading else { return }
        let now = Date()
        if now.timeIntervalSince(trustCheckedAt) >= 2 {
            trustCheckedAt = now
            trusted = AXIsProcessTrusted()
        }
        // Without Accessibility neither the controls nor the shortcuts can be
        // read, and a saved state alone would go stale, so Discord is unknown.
        let app = trusted ? DiscordVoiceReader.App.running() : nil
        if app == nil { shortcuts.stop() } else { shortcuts.start() }
        let readsAudio = now.timeIntervalSince(audioReadAt) >= 1
        if readsAudio { audioReadAt = now }
        let micMute = MicMuteService.shared.isMuted
        let reader = reader
        let tracker = shortcuts.tracker
        let generation = generation
        reading = true
        queue.async { [weak self] in
            let discord = reader.read(app, tracker: tracker)
            let audio = readsAudio ? VoiceAudioProbe.read(micMuteActive: micMute) : nil
            DispatchQueue.main.async {
                guard let self, self.generation == generation else { return }
                self.reading = false
                let now = Date()
                let since = self.discordSince
                if discord == nil { self.discordSince = nil } else if since == nil { self.discordSince = now }
                var next = self.snapshot
                next.discord = discord
                next.settled = since.map { now.timeIntervalSince($0) >= Self.settle } ?? false
                if let audio {
                    next.recording = audio.recording
                    next.systemMuted = audio.systemMuted
                }
                if next != self.snapshot { self.snapshot = next }
            }
        }
    }
}

/// The shortcut state, shared by the event monitors on the main thread and
/// the reader on its own queue.
final class DiscordShortcutTrackerBox {
    private let lock = NSLock()
    private var tracker = DiscordShortcutTracker()

    func with<Result>(_ body: (inout DiscordShortcutTracker) -> Result) -> Result {
        lock.lock()
        defer { lock.unlock() }
        return body(&tracker)
    }

    /// Forgets everything learned, keeping whether shortcuts are followed.
    func reset() {
        with { tracker in
            let active = tracker.active
            tracker = DiscordShortcutTracker()
            tracker.active = active
        }
    }
}

/// Watches keys and mouse buttons only to see Discord's own shortcuts:
/// listen-only monitors that never consume, change or forward an event.
final class DiscordShortcutMonitor {
    let tracker = DiscordShortcutTrackerBox()
    private var monitors: [Any] = []
    private var observers: [(NotificationCenter, NSObjectProtocol)] = []
    private var pressed = Set<UInt16>()

    private static let events: NSEvent.EventTypeMask = [
        .keyDown, .keyUp, .flagsChanged, .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp,
        .otherMouseDown, .otherMouseUp,
    ]

    func start() {
        guard monitors.isEmpty else { return }
        tracker.with { $0.active = true }
        if let global = NSEvent.addGlobalMonitorForEvents(matching: Self.events, handler: { [weak self] in self?.handle($0) }) {
            monitors.append(global)
        }
        // Keys typed into Vorssaint itself never reach a global monitor.
        if let local = NSEvent.addLocalMonitorForEvents(matching: Self.events, handler: { [weak self] event in
            self?.handle(event)
            return event
        }) {
            monitors.append(local)
        }
        // A key released while the screen is locked or the session is away
        // reaches no monitor; held, it would keep a push-to-mute muted.
        let release: (Notification) -> Void = { [weak self] _ in self?.releaseKeys() }
        let workspace = NSWorkspace.shared.notificationCenter
        let distributed = DistributedNotificationCenter.default()
        observers = [
            (workspace, workspace.addObserver(forName: NSWorkspace.sessionDidResignActiveNotification, object: nil,
                                              queue: .main, using: release)),
            (workspace, workspace.addObserver(forName: NSWorkspace.screensDidSleepNotification, object: nil,
                                              queue: .main, using: release)),
            (distributed, distributed.addObserver(forName: Notification.Name("com.apple.screenIsLocked"), object: nil,
                                                  queue: .main, using: release)),
        ]
    }

    private func releaseKeys() {
        pressed.removeAll()
        tracker.with { $0.sample({ _ in false }, now: Date()) }
    }

    func stop() {
        guard !monitors.isEmpty else { return }
        monitors.forEach(NSEvent.removeMonitor)
        monitors.removeAll()
        observers.forEach { $0.0.removeObserver($0.1) }
        observers.removeAll()
        pressed.removeAll()
        tracker.with { $0 = DiscordShortcutTracker() }
    }

    private func handle(_ event: NSEvent) {
        switch event.type {
        case .keyDown: pressed.insert(event.keyCode)
        case .keyUp: pressed.remove(event.keyCode)
        default: break
        }
        let modifiers = DiscordKeybinds.modifierKeys(rawFlags: event.modifierFlags.rawValue)
        let buttons = NSEvent.pressedMouseButtons
        let keys = pressed
        tracker.with { tracker in
            tracker.sample({ key in
                switch key {
                case .key(let code): return modifiers.contains(code) || keys.contains(code)
                case .mouse(let button): return buttons & (1 << button) != 0
                }
            }, now: Date())
        }
    }
}

/// Reads Discord on the service's queue only.
final class DiscordVoiceReader {
    struct App: Equatable {
        let pid: pid_t
        let profile: String
        let hidden: Bool
        let frontmost: Bool

        /// The first Discord edition running; reading it on the main thread
        /// keeps the list in step with the workspace's notifications.
        static func running() -> App? {
            let frontmost = NSWorkspace.shared.frontmostApplication?.processIdentifier
            for app in NSWorkspace.shared.runningApplications where !app.isTerminated {
                guard let id = app.bundleIdentifier, let profile = DiscordVoiceSupport.editions[id] else { continue }
                return App(pid: app.processIdentifier, profile: profile, hidden: app.isHidden,
                           frontmost: app.processIdentifier == frontmost)
            }
            return nil
        }
    }

    private struct Controls {
        let pid: pid_t
        let window: AXUIElement
        let microphone: AXUIElement
        let headphones: AXUIElement
    }

    private var voice = DiscordStorage.voiceReader()
    private var keybinds = DiscordStorage.keybindReader()
    private var keybindsReadAt: Date?
    private var profile: String?
    private var lastControls: DiscordVoiceState?
    private var controls: Controls?
    private var searchedAt: Date?
    private var accessiblePIDs = Set<pid_t>()

    /// A visible window's tree is searched again at most this often while
    /// its controls cannot be found, as on a settings page.
    private static let searchInterval: TimeInterval = 1
    private static let maximumElements = 8000

    func reset() {
        voice = DiscordStorage.voiceReader()
        keybinds = DiscordStorage.keybindReader()
        keybindsReadAt = nil
        profile = nil
        lastControls = nil
        controls = nil
        searchedAt = nil
    }

    func read(_ app: App?, tracker: DiscordShortcutTrackerBox) -> DiscordVoiceState? {
        guard let app else {
            controls = nil
            profile = nil
            lastControls = nil
            accessiblePIDs.removeAll()
            return nil
        }
        let now = Date()
        if profile != app.profile {
            tracker.reset()
            profile = app.profile
            keybindsReadAt = nil
            lastControls = nil
        }
        // Live controls come first. The saved state can stay as it was after
        // a click, so it must never override them.
        if let state = readControls(app, now: now) {
            let tracked = tracker.with { $0.state }
            if app.frontmost || lastControls != state || tracked == nil {
                if tracker.with({ $0.live(state, now: now) }) { lastControls = state }
            }
        }
        let directory = DiscordStorage.directory(profile: app.profile)
        if keybindsReadAt.map({ now.timeIntervalSince($0) >= 2 }) ?? true {
            // A table LevelDB is still writing reads as nothing for a moment;
            // the shortcuts last read stay until the store reads again.
            if let bindings = keybinds.read(directory) { tracker.with { $0.setBindings(bindings) } }
            keybindsReadAt = now
        }
        // A seed for the start only: a saved snapshot is not a live signal
        // and cannot undo a click or a shortcut.
        if tracker.with({ $0.state }) == nil, let saved = voice.read(directory) {
            tracker.with { $0.seed(saved) }
        }
        return tracker.with { $0.state }
    }

    // MARK: Accessibility

    private func readControls(_ app: App, now: Date) -> DiscordVoiceState? {
        // The tree is only trusted for a window on screen.
        guard !app.hidden else { controls = nil; return nil }
        let application = AXUIElementCreateApplication(app.pid)
        AXUIElementSetMessagingTimeout(application, 0.3)
        if !accessiblePIDs.contains(app.pid) {
            // Discord, like every Chromium app, builds its accessibility tree
            // only when asked; this asks without changing anything it shows.
            AXUIElementSetAttributeValue(application, "AXManualAccessibility" as CFString, kCFBooleanTrue)
            accessiblePIDs.insert(app.pid)
        }
        if let controls, controls.pid == app.pid, Self.isOnScreen(controls.window),
           let state = Self.state(microphone: controls.microphone, headphones: controls.headphones) {
            return state
        }
        controls = nil
        if let searchedAt, now.timeIntervalSince(searchedAt) < Self.searchInterval { return nil }
        searchedAt = now
        let windows = (Self.attribute(application, kAXWindowsAttribute) as? [AXUIElement] ?? []).filter(Self.isOnScreen)
        for window in windows {
            var microphone: AXUIElement?
            var headphones: AXUIElement?
            var budget = Self.maximumElements
            Self.search(window, budget: &budget) { element in
                guard let control = Self.control(element) else { return false }
                if control.headphones { headphones = element } else { microphone = element }
                return microphone != nil && headphones != nil
            }
            if let microphone, let headphones,
               let state = Self.state(microphone: microphone, headphones: headphones) {
                controls = Controls(pid: app.pid, window: window, microphone: microphone, headphones: headphones)
                searchedAt = nil
                return state
            }
        }
        return nil
    }

    private static func state(microphone: AXUIElement, headphones: AXUIElement) -> DiscordVoiceState? {
        guard let mic = control(microphone), !mic.headphones, let phones = control(headphones), phones.headphones
        else { return nil }
        return DiscordVoiceState(muted: mic.muted || phones.muted, deafened: phones.muted)
    }

    private static func control(_ element: AXUIElement) -> (headphones: Bool, muted: Bool)? {
        let role = attribute(element, kAXRoleAttribute) as? String
        guard role == kAXCheckBoxRole || role == kAXButtonRole else { return nil }
        let name = [kAXDescriptionAttribute, kAXTitleAttribute].lazy
            .compactMap { attribute(element, $0) as? String }.first { !$0.isEmpty } ?? ""
        guard DiscordVoiceSupport.control(name, checked: nil) != nil else { return nil }
        // A switch or toggle reports its state as 0 or 1; a plain button only by its label.
        let checked = (attribute(element, kAXValueAttribute) as? NSNumber).flatMap { value -> Bool? in
            switch value.intValue {
            case 0: return false
            case 1: return true
            default: return nil
            }
        }
        return DiscordVoiceSupport.control(name, checked: checked)
    }

    /// Depth first, stopping once `found` says the search is done or the budget runs out.
    @discardableResult
    private static func search(_ element: AXUIElement, budget: inout Int, found: (AXUIElement) -> Bool) -> Bool {
        guard budget > 0 else { return true }
        budget -= 1
        if found(element) { return true }
        for child in attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? [] {
            if search(child, budget: &budget, found: found) { return true }
        }
        return false
    }

    private static func isOnScreen(_ window: AXUIElement) -> Bool {
        attribute(window, kAXMinimizedAttribute) as? Bool != true
    }

    private static func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
        var value: AnyObject?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
        return value
    }
}

/// Whether some app records from an input, and whether the Mac's mute
/// silences what records. Only device and process properties are read.
enum VoiceAudioProbe {
    static func read(micMuteActive: Bool) -> (recording: Bool, systemMuted: Bool) {
        let recording = inputIsRecording()
        guard recording else { return (false, false) }
        if micMuteActive { return (true, true) }
        let running = inputDevices().filter { uint32(of: $0, kAudioDevicePropertyDeviceIsRunningSomewhere) == 1 }
        let muted = !running.isEmpty && running.allSatisfy {
            uint32(of: $0, kAudioDevicePropertyMute, scope: kAudioObjectPropertyScopeInput) == 1
        }
        return (true, muted)
    }

    /// Process objects say exactly who records; before macOS 14.2 nothing is
    /// known to be recording, so the Mac's mute is not shown.
    private static func inputIsRecording() -> Bool {
        guard #available(macOS 14.2, *) else { return false }
        return objects(kAudioHardwarePropertyProcessObjectList).contains {
            uint32(of: $0, kAudioProcessPropertyIsRunningInput) == 1
        }
    }

    private static func inputDevices() -> [AudioObjectID] {
        objects(kAudioHardwarePropertyDevices).filter { device in
            var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreams,
                                                     mScope: kAudioObjectPropertyScopeInput,
                                                     mElement: kAudioObjectPropertyElementMain)
            var size: UInt32 = 0
            return AudioObjectGetPropertyDataSize(device, &address, 0, nil, &size) == noErr && size > 0
        }
    }

    private static func objects(_ selector: AudioObjectPropertySelector) -> [AudioObjectID] {
        var address = AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        let system = AudioObjectID(kAudioObjectSystemObject)
        guard AudioObjectGetPropertyDataSize(system, &address, 0, nil, &size) == noErr, size > 0 else { return [] }
        var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(system, &address, 0, nil, &size, &ids) == noErr else { return [] }
        return Array(ids.prefix(Int(size) / MemoryLayout<AudioObjectID>.size))
    }

    private static func uint32(of object: AudioObjectID, _ selector: AudioObjectPropertySelector,
                               scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> UInt32? {
        var address = AudioObjectPropertyAddress(mSelector: selector, mScope: scope,
                                                 mElement: kAudioObjectPropertyElementMain)
        guard AudioObjectHasProperty(object, &address) else { return nil }
        var value: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        guard AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value) == noErr else { return nil }
        return value
    }
}
