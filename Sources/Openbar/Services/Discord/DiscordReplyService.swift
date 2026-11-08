// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import ApplicationServices

/// Sends a reply to a Discord notification through the person's running
/// Discord app: it brings Discord forward, finds the message box of the
/// conversation the notification names through accessibility, checks the box
/// is empty, types the reply and presses Return. No account token is read and
/// nothing goes over the network from Openbar. Every check that fails stops
/// before Return, and the island keeps the draft.
enum DiscordReplyService {
    typealias Failure = DiscordReplySupport.Failure

    /// The app replies go to. A stand-in can be named with
    /// `-OpenbarDiscordReplyBundleID <id>` for tests, so they never drive the
    /// person's own Discord.
    static var bundleIdentifier: String {
        UserDefaults.standard.string(forKey: "OpenbarDiscordReplyBundleID") ?? DiscordReplySupport.stableBundleIdentifier
    }

    private static let queue = DispatchQueue(label: "com.openbar.discord-reply", qos: .userInitiated)
    private static let maximumElements = 6_000
    private static let returnKey: CGKeyCode = 36

    /// Runs off the main thread and answers on it.
    static func send(title: String, text: String, completion: @escaping (Failure?) -> Void) {
        queue.async {
            let failure: Failure?
            do { try send(title: title, text: text); failure = nil } catch let error as Failure { failure = error }
            catch { failure = .unavailable }
            DispatchQueue.main.async { completion(failure) }
        }
    }

    private static func send(title: String, text: String) throws {
        guard let target = DiscordReplySupport.target(title) else { throw Failure.noChat }
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, text.count <= DiscordReplySupport.limit else { throw Failure.tooLong }
        guard AXIsProcessTrusted() else { throw Failure.inspectFailed }
        let app = try bringForward()
        let pid = app.processIdentifier
        let application = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(application, 0.3)
        // Discord, like every Chromium app, builds its accessibility tree only
        // when asked; this asks without changing anything it shows.
        AXUIElementSetAttributeValue(application, "AXManualAccessibility" as CFString, kCFBooleanTrue)

        // Chromium fills in a page's tree a moment after it is first asked,
        // and only for a window in front, so the search waits for it; every
        // window is searched, the focused one first.
        var found: (editor: AXUIElement, label: String, window: String)?
        for _ in 0..<24 where found == nil {
            for window in windows(of: application) {
                let windowTitle = attribute(window, kAXTitleAttribute) as? String ?? ""
                if let match = composer(in: window, fits: {
                    DiscordReplySupport.matches(target, composer: $0, window: windowTitle)
                }) {
                    found = (match.0, match.1, windowTitle)
                    break
                }
            }
            if found == nil { Thread.sleep(forTimeInterval: 0.25) }
        }
        guard let found else { throw Failure.wrongChat }
        // Return would send a half-written message there together with the reply.
        guard let existing = attribute(found.editor, kAXValueAttribute) as? String else { throw Failure.inspectFailed }
        guard DiscordReplySupport.isBlank(existing, label: found.label) else { throw Failure.draftInDiscord }
        AXUIElementSetAttributeValue(found.editor, kAXFocusedAttribute as CFString, kCFBooleanTrue)
        Thread.sleep(forTimeInterval: 0.08)
        guard isFocused(found.editor, in: application), isFrontmost(pid) else { throw Failure.focusLost }
        try type(text, into: pid)
        Thread.sleep(forTimeInterval: 0.12)
        guard isFrontmost(pid), isFocused(found.editor, in: application) else { throw Failure.typingInterrupted }
        guard post(key: returnKey, flags: []) else { throw Failure.inputRefused }
    }

    /// Discord's app, launched if needed, in front with a window.
    private static func bringForward() throws -> NSRunningApplication {
        let identifier = bundleIdentifier
        var app = NSRunningApplication.runningApplications(withBundleIdentifier: identifier).first
        if app == nil {
            guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: identifier) else { throw Failure.openFailed }
            let launched = DispatchSemaphore(value: 0)
            NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration()) { opened, _ in
                app = opened; launched.signal()
            }
            _ = launched.wait(timeout: .now() + 10)
        }
        guard let app else { throw Failure.openFailed }
        app.activate(options: [.activateAllWindows])
        for attempt in 0..<40 {
            if isFrontmost(app.processIdentifier), hasWindow(app.processIdentifier) { return app }
            // Discord closed to the tray has no window to activate; asking the
            // running app to open, as the Dock does, brings its window back.
            if attempt == 8, !hasWindow(app.processIdentifier), let url = app.bundleURL {
                let configuration = NSWorkspace.OpenConfiguration()
                configuration.activates = true
                configuration.createsNewApplicationInstance = false
                NSWorkspace.shared.openApplication(at: url, configuration: configuration) { _, _ in }
            }
            Thread.sleep(forTimeInterval: 0.1)
        }
        throw Failure.focusLost
    }

    /// The text area whose label `fits`: Discord's message box. Its label is
    /// the placeholder, which Chromium reports as the description.
    private static func composer(in window: AXUIElement, fits: (String) -> Bool) -> (AXUIElement, String)? {
        var queue = [window]
        var index = 0
        while index < queue.count, index < maximumElements {
            let element = queue[index]
            index += 1
            let role = attribute(element, kAXRoleAttribute) as? String
            if role == kAXTextAreaRole || role == kAXTextFieldRole {
                for name in [kAXDescriptionAttribute, kAXTitleAttribute, kAXPlaceholderValueAttribute] {
                    if let label = attribute(element, name) as? String, fits(label) { return (element, label) }
                }
                continue
            }
            if let children = attribute(element, kAXChildrenAttribute) as? [AXUIElement] { queue.append(contentsOf: children) }
        }
        return nil
    }

    private static func hasWindow(_ pid: pid_t) -> Bool {
        !((attribute(AXUIElementCreateApplication(pid), kAXWindowsAttribute) as? [AXUIElement]) ?? []).isEmpty
    }

    private static func windows(of application: AXUIElement) -> [AXUIElement] {
        var windows = (attribute(application, kAXWindowsAttribute) as? [AXUIElement]) ?? []
        if let value = attribute(application, kAXFocusedWindowAttribute), CFGetTypeID(value) == AXUIElementGetTypeID() {
            let focused = unsafeBitCast(value, to: AXUIElement.self)
            windows.removeAll { CFEqual($0, focused) }
            windows.insert(focused, at: 0)
        }
        return windows
    }

    private static func isFocused(_ editor: AXUIElement, in application: AXUIElement) -> Bool {
        guard let focused = attribute(application, kAXFocusedUIElementAttribute) else { return false }
        return CFEqual(focused, editor)
    }

    private static func isFrontmost(_ pid: pid_t) -> Bool {
        NSWorkspace.shared.frontmostApplication?.processIdentifier == pid
    }

    /// Typed, not pasted, so the clipboard stays as it was. Discord must stay
    /// in front: it is checked before each chunk, so a switch mid-reply stops
    /// the rest from landing in another app.
    private static func type(_ text: String, into pid: pid_t) throws {
        var typed = false
        for (number, line) in text.components(separatedBy: "\n").enumerated() {
            if number > 0 {
                guard isFrontmost(pid) else { throw Failure.typingInterrupted }
                guard post(key: returnKey, flags: .maskShift) else { throw Failure.inputRefused }
            }
            let units = Array(line.utf16)
            var index = 0
            while index < units.count {
                guard isFrontmost(pid) else { throw typed ? Failure.typingInterrupted : Failure.focusLost }
                var end = min(index + 20, units.count)
                if end < units.count, UTF16.isLeadSurrogate(units[end - 1]) { end -= 1 }
                let chunk = Array(units[index..<end])
                for down in [true, false] {
                    guard let event = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down) else {
                        throw typed ? Failure.typingInterrupted : Failure.inputRefused
                    }
                    event.flags = []
                    event.keyboardSetUnicodeString(stringLength: chunk.count, unicodeString: chunk)
                    event.post(tap: .cghidEventTap)
                }
                typed = true
                index = end
                Thread.sleep(forTimeInterval: 0.01)
            }
        }
    }

    private static func post(key: CGKeyCode, flags: CGEventFlags) -> Bool {
        for down in [true, false] {
            guard let event = CGEvent(keyboardEventSource: nil, virtualKey: key, keyDown: down) else { return false }
            event.flags = flags
            event.post(tap: .cghidEventTap)
        }
        return true
    }

    private static func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
        AXUIElementSetMessagingTimeout(element, 0.3)
        var value: CFTypeRef?
        return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
    }
}
