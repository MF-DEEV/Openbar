// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import Carbon.HIToolbox
import Combine
import SwiftUI

/// Sessions in the menu bar (#331): an item of their own that tells whether
/// a session waits on the person, and a panel under it with the island's
/// Sessions page. It works next to the island or without it, and keeps the
/// sessions followed while the island is turned off.
final class SessionsMenuBar: ObservableObject {
    static let shared = SessionsMenuBar()

    @Published private(set) var panelShown = false
    /// The panel grown around a session's page, as the island enlarges.
    @Published private(set) var enlarged = false
    /// The panel's size as placed, which the page inside is laid out in.
    @Published private(set) var panelSize = SessionsMenuBarSupport.panelSize
    /// Focus rings show once the keyboard is used, as in the island.
    @Published private(set) var showsKeyboardFocus = false
    /// How tall the list stands on its own; nil while a page fills the panel.
    private var listHeight: CGFloat?

    private var item: SessionsStatusItem?
    private var panel: SessionsMenuBarPanel?
    private var bindings = Set<AnyCancellable>()
    /// Follows every settings change while the app runs.
    private var settingsObserver: NSObjectProtocol?
    /// What the item listens to while it is in the bar.
    private var observers: [(NotificationCenter, NSObjectProtocol)] = []
    private var settingsSyncScheduled = false
    private var terminating = false
    /// New asks pulse the icon once; the ones already waiting at the start
    /// stay still.
    private var pulseAttention = NotchSessionAttention()
    private var pulseBaselined = false
    /// Asks the panel opens on by itself, when the person allows it (#167).
    private var openAttention = NotchSessionAttention()
    private var presentingAllowed = false
    private var presentingWork: DispatchWorkItem?
    /// The page's innermost layer, which Escape closes first.
    private var pageLayer: (() -> Void)?
    private var outsideClicks: Any?
    private let voiceKey = QuickToolHotkey(id: 86)
    private let allowKey = QuickToolHotkey(id: 87)
    private let denyKey = QuickToolHotkey(id: 88)
    private let alwaysKey = QuickToolHotkey(id: 89)
    private var keysWork: DispatchWorkItem?

    private init() {}

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(L10n.shared.language) }

    /// The menu bar shows the sessions, with or without the island.
    var followsSessions: Bool { !terminating && NotchClaudeSessionsSupport.inMenuBar() }

    // MARK: Life

    /// At launch: follows the settings from now on.
    func start() {
        guard settingsObserver == nil else { return }
        settingsObserver = NotificationCenter.default.addObserver(forName: UserDefaults.didChangeNotification,
                                                                  object: nil, queue: .main) { [weak self] _ in
            self?.scheduleSettingsSync()
        }
        sync()
    }

    /// Quitting: the item goes, and the sessions stop unless the island
    /// still holds them, which it stops itself.
    func terminate() {
        terminating = true
        tearDown()
        if !NotchService.shared.running {
            AgentUsageService.shared.stop()
            ClaudeSessionService.shared.stop()
            SessionAlerts.shared.stop()
        }
    }

    private func scheduleSettingsSync() {
        guard !settingsSyncScheduled else { return }
        settingsSyncScheduled = true
        // Showing an item writes its remembered place into the same domain
        // and announces it while that work is still on the stack.
        DispatchQueue.main.async { [weak self] in
            self?.settingsSyncScheduled = false
            self?.sync()
        }
    }

    func sync() {
        guard followsSessions else { tearDown(); return }
        // Without the island the menu bar keeps the sessions, the limits on
        // their rows and their sounds going; with it, the island does.
        if !NotchService.shared.running {
            ClaudeSessionService.shared.syncWithPreferences()
            AgentUsageService.shared.syncWithPreferences()
        }
        SessionAlerts.shared.sync()
        if item == nil { install() }
        refreshItem()
        scheduleKeysSync()
    }

    private func install() {
        let item = SessionsStatusItem()
        item.onClick = { [weak self] in self?.togglePanel() }
        item.onRightClick = { [weak self] in self?.showMenu() }
        self.item = item
        let service = ClaudeSessionService.shared
        service.$sessions
            .receive(on: DispatchQueue.main)
            .sink { [weak self] sessions in self?.sessionsChanged(sessions) }
            .store(in: &bindings)
        service.$focusedID.combineLatest(service.$dressingID)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in self?.scheduleKeysSync() }
            .store(in: &bindings)
        // Back on the list, the panel goes back to its size, as the island does.
        service.$focusedID
            .removeDuplicates()
            .receive(on: DispatchQueue.main)
            .sink { [weak self] id in if id == nil { self?.setEnlarged(false) } }
            .store(in: &bindings)
        L10n.shared.$language
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in self?.refreshItem() }
            .store(in: &bindings)
        // Another layout moves the letters, so the keys move with them.
        NotificationCenter.default.publisher(for: GlobalShortcut.keyboardLayoutDidChange)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in self?.scheduleKeysSync() }
            .store(in: &bindings)
        let workspace = NSWorkspace.shared.notificationCenter
        observe(workspace, NSWorkspace.didActivateApplicationNotification) { [weak self] in self?.syncPresenting() }
        observe(DistributedNotificationCenter.default(), Notification.Name("com.apple.screenIsLocked")) { [weak self] in
            self?.presentingAllowed = false
            self?.closePanel()
        }
        voiceKey.onPress = { VoiceReplyRecorder.shared.shortcutPressed() }
        voiceKey.onRelease = { VoiceReplyRecorder.shared.shortcutReleased() }
        allowKey.onPress = { [weak self] in self?.answer(.allow) }
        denyKey.onPress = { [weak self] in self?.answer(.deny) }
        alwaysKey.onPress = { [weak self] in self?.answer(.allowAlways) }
        syncPresenting()
    }

    private func tearDown() {
        closePanel()
        bindings.removeAll()
        observers.forEach { $0.0.removeObserver($0.1) }
        observers.removeAll()
        keysWork?.cancel(); keysWork = nil
        presentingWork?.cancel(); presentingWork = nil
        presentingAllowed = false
        voiceKey.unregister()
        allowKey.unregister()
        denyKey.unregister()
        alwaysKey.unregister()
        item?.remove()
        item = nil
        pulseAttention = NotchSessionAttention()
        pulseBaselined = false
        openAttention = NotchSessionAttention()
        // Turned off here with the island off too: the sessions, their
        // limits and their sounds stop when nothing shows them.
        if !terminating, !NotchService.shared.running {
            ClaudeSessionService.shared.syncWithPreferences()
            AgentUsageService.shared.syncWithPreferences()
            SessionAlerts.shared.sync()
        }
    }

    private func observe(_ center: NotificationCenter, _ name: Notification.Name, action: @escaping () -> Void) {
        let token = center.addObserver(forName: name, object: nil, queue: .main) { _ in action() }
        observers.append((center, token))
    }

    // MARK: Item

    private func refreshItem() {
        let waiting = ClaudeSessionService.shared.counts.waiting
        item?.update(waiting: waiting, text: text)
    }

    /// A new ask pulses the icon and is read out; the panel opens on it by
    /// itself only when the person asked for that and is at the desktop or
    /// in Openbar, under the island's own rule (#167).
    private func sessionsChanged(_ sessions: [ClaudeSession]) {
        refreshItem()
        let waiting = Dictionary(sessions.compactMap { session in
            NotchSessionAttention.request(of: session).map { (session.id, $0) }
        }, uniquingKeysWith: { first, _ in first })
        let fresh = pulseAttention.sync(waiting, allowed: true)
        if pulseBaselined, let first = fresh.first, let session = sessions.first(where: { $0.id == first }) {
            item?.pulse()
            announce(text.menuBarAnnouncement(ClaudeSessionService.shared.displayName(session)))
        }
        pulseBaselined = true
        presentPending(waiting, sessions: sessions)
        scheduleKeysSync()
    }

    /// Opens the panel on what waits, when it may (#167): only the menu
    /// bar shows the sessions, the person turned it on, and the desktop or
    /// Openbar has been in front a moment.
    private func presentPending(_ waiting: [String: NotchSessionAttention.Request], sessions: [ClaudeSession]) {
        guard !NotchClaudeSessionsSupport.inIsland(), NotchClaudeSessionsSupport.menuBarOpens() else { return }
        // An ask on screen in the open panel counts as shown.
        let ids = openAttention.sync(waiting, allowed: presentingAllowed || panelShown)
        guard !panelShown, let first = ids.first else { return }
        // More than one ask waiting opens on the queue, not on one of them (#269).
        ClaudeSessionService.shared.focus(NotchPermissionQueue(sessions).shows ? nil : first)
        openPanel(makeKey: false)
    }

    /// Follows what is in front, as the island does: the desktop or
    /// Openbar allow opening after a moment; any other app holds it at once.
    private func syncPresenting() {
        let front = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
        guard front != AssistiveKeyboard.bundleID else { return }
        guard NotchSessionAttention.allowsPresenting(frontmost: front, own: Bundle.main.bundleIdentifier) else {
            presentingWork?.cancel(); presentingWork = nil
            presentingAllowed = false
            return
        }
        guard !presentingAllowed, presentingWork == nil else { return }
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.presentingWork = nil
            self.presentingAllowed = true
            self.sessionsChanged(ClaudeSessionService.shared.sessions)
        }
        presentingWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + NotchClaudeSessionsSupport.presentingDelay, execute: work)
    }

    /// A reminder (#231), after its sound: the icon pulses, VoiceOver says
    /// who waits, and the panel opens where it may.
    func remind(_ waits: [NotchSessionReminders.Wait], sessions: [ClaudeSession]) {
        guard followsSessions, let first = waits.first,
              let session = sessions.first(where: { $0.id == first.session }) else { return }
        item?.pulse()
        announce(text.menuBarAnnouncement(ClaudeSessionService.shared.displayName(session)))
        guard !NotchClaudeSessionsSupport.inIsland(), NotchClaudeSessionsSupport.menuBarOpens(),
              presentingAllowed, !panelShown else { return }
        ClaudeSessionService.shared.focus(waits.count == 1 ? first.session : nil)
        openPanel(makeKey: false)
    }

    private func announce(_ message: String) {
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested,
                             userInfo: [.announcement: message,
                                        .priority: NSAccessibilityPriorityLevel.high.rawValue])
    }

    // MARK: Menu

    /// A right click: what waits, each ask with Allow and Deny where the
    /// island's queue offers them, then the panel and the settings.
    private func showMenu() {
        closePanel()
        let service = ClaudeSessionService.shared
        let menu = NSMenu()
        menu.autoenablesItems = false
        let header = NSMenuItem(title: text.menuBarWaiting(service.counts.waiting), action: nil, keyEquivalent: "")
        header.isEnabled = false
        menu.addItem(header)
        let asks = SessionsMenuBarSupport.menuAsks(service.sessions) { service.displayName($0) }
        if !asks.isEmpty { menu.addItem(.separator()) }
        for ask in asks {
            let entry = NSMenuItem(title: SessionsMenuBarSupport.menuTitle(ask.title), action: nil, keyEquivalent: "")
            if #available(macOS 14.4, *), let risk = ask.risk, let reason = text.risk(risk) { entry.subtitle = reason }
            let choices = NSMenu()
            if ask.decides {
                choices.addItem(menuItem(text.allow) { service.decide(ask.session, request: ask.request, .allow) })
                choices.addItem(menuItem(text.deny) { service.decide(ask.session, request: ask.request, .deny) })
                choices.addItem(.separator())
            }
            choices.addItem(menuItem(text.menuBarOpenAsk) { [weak self] in
                service.focus(ask.session, request: ask.request)
                self?.openPanel(makeKey: true)
            })
            entry.submenu = choices
            menu.addItem(entry)
        }
        menu.addItem(.separator())
        menu.addItem(menuItem(text.menuBarOpenSessions) { [weak self] in self?.openPanel(makeKey: true) })
        menu.addItem(menuItem(text.menuBarSettings) { NotchService.shared.openSettings(showing: .sessions) })
        item?.popUp(menu)
    }

    private func menuItem(_ title: String, action: @escaping () -> Void) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: #selector(SessionsMenuAction.run), keyEquivalent: "")
        let target = SessionsMenuAction(action)
        item.target = target
        item.representedObject = target
        return item
    }

    // MARK: Panel

    func togglePanel() {
        if panelShown { closePanel() } else { openPanel(makeKey: true) }
    }

    /// Opens the panel under the item. Taking the keyboard is for a click;
    /// opened by itself, it leaves the keyboard where it is.
    func openPanel(makeKey: Bool) {
        guard followsSessions else { return }
        let panel = self.panel ?? makePanel()
        let opening = !panelShown
        // Shown before the island lets go, so the island's page leaves the
        // session it showed to the panel rather than closing it.
        panelShown = true
        // One place shows the sessions at a time: the island's page closes.
        let notch = NotchService.shared
        if notch.expanded, notch.selected == .sessions { notch.collapse() }
        if opening { showsKeyboardFocus = false }
        let frame = place(panel)
        if opening {
            panel.setContent(SessionsMenuBarPanelView())
            item?.setHighlighted(true)
            installMonitors()
        }
        panel.setFrame(frame, display: true)
        panel.orderFrontRegardless()
        if makeKey { panel.makeKey() }
        scheduleKeysSync()
    }

    func closePanel() {
        guard panelShown else { return }
        panelShown = false
        enlarged = false
        pageLayer = nil
        removeMonitors()
        item?.setHighlighted(false)
        panel?.orderOut(nil)
        // The page leaves with the panel, so it lets go of its session as
        // the island's does when it closes.
        panel?.clearContent()
        scheduleKeysSync()
    }

    func setEnlarged(_ on: Bool) {
        guard enlarged != on else { return }
        enlarged = on
        guard panelShown, let panel else { return }
        let frame = place(panel)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 0.22
            panel.animator().setFrame(frame, display: true)
        }
    }

    /// The panel fits a short list rather than hanging empty below it, and
    /// takes its full size for a session's page. Its top stays under the item.
    func fitList(_ height: CGFloat?) {
        guard listHeight != height else { return }
        listHeight = height
        guard panelShown, let panel else { return }
        panel.setFrame(place(panel), display: true)
    }

    /// Escape steps back from the page's innermost layer first.
    func setPageLayer(_ close: (() -> Void)?) { pageLayer = close }

    /// The session's page, or the list holding it, is open in the panel.
    func showsSessionInView(_ session: String) -> Bool {
        guard panelShown else { return false }
        let focused = ClaudeSessionService.shared.focusedID
        return focused == nil || focused == session
    }

    private func makePanel() -> SessionsMenuBarPanel {
        let panel = SessionsMenuBarPanel()
        panel.setAccessibilityTitle(text.menuBarItemName)
        panel.onKey = { [weak self] event in self?.handleKey(event) ?? false }
        panel.onKeyChange = { [weak self] in self?.scheduleKeysSync() }
        self.panel = panel
        return panel
    }

    /// Under the item, on the screen it is on; at the right end of the menu
    /// bar of the pointer's screen when macOS has not placed it or the notch
    /// hides it.
    private func place(_ panel: NSPanel) -> CGRect {
        let full = enlarged ? SessionsMenuBarSupport.enlargedSize : SessionsMenuBarSupport.panelSize
        let wanted = CGSize(width: full.width,
                            height: SessionsMenuBarSupport.fittedHeight(list: enlarged ? nil : listHeight, full: full.height))
        let itemFrame = item?.trustedFrame
        let screen = itemFrame.flatMap { frame in NSScreen.screens.first { $0.frame.intersects(frame) } }
            ?? NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) } ?? NSScreen.main
        let visible = screen?.visibleFrame ?? CGRect(x: 0, y: 0, width: 1440, height: 875)
        let frame = SessionsMenuBarSupport.frame(size: wanted, under: itemFrame, visible: visible)
        if panelSize != frame.size { panelSize = frame.size }
        return frame
    }

    private func installMonitors() {
        removeMonitors()
        // A click in another app or on the desktop closes the panel; the
        // item's own click toggles it.
        outsideClicks = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
            guard let self, self.panelShown else { return }
            if self.item?.contains(NSEvent.mouseLocation) == true { return }
            self.closePanel()
        }
    }

    private func removeMonitors() {
        if let outsideClicks { NSEvent.removeMonitor(outsideClicks) }
        outsideClicks = nil
    }

    // MARK: Keys

    /// Keys the panel reads while it has the keyboard: Escape steps back,
    /// ⌘W closes, ⌘Y, ⌘N and ⌘T answer the request on screen.
    private func handleKey(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection([.command, .control, .option, .shift])
        if [kVK_Tab, kVK_UpArrow, kVK_DownArrow, kVK_LeftArrow, kVK_RightArrow].contains(Int(event.keyCode)),
           !showsKeyboardFocus {
            showsKeyboardFocus = true
        }
        if Int(event.keyCode) == kVK_Escape, flags.isEmpty {
            // Text being composed keeps its own Escape.
            if let editor = panel?.firstResponder as? NSTextView, editor.hasMarkedText() { return false }
            if VoiceReplyRecorder.shared.cancel() { return true }
            if let close = pageLayer { close(); return true }
            closePanel()
            return true
        }
        if flags == .command, event.charactersIgnoringModifiers?.lowercased() == "w" {
            closePanel()
            return true
        }
        guard let decision = NotchClaudeSessionsSupport.decision(key: event.charactersIgnoringModifiers,
                                                                 commandOnly: flags == .command) else { return false }
        // ⌘T held down turns Always over once, not on and off as it repeats.
        if event.isARepeat, decision == .allowAlways { return keyTarget(decision) != nil }
        return answer(decision)
    }

    private func keyTarget(_ decision: ClaudeHookMessage.Decision)
        -> (id: String, request: String, provider: ClaudeSessionProvider)? {
        guard panelShown else { return nil }
        let service = ClaudeSessionService.shared
        return NotchClaudeSessionsSupport.keyTarget(decision, focused: service.focusedSession,
                                                    sessions: service.sessions, dressing: service.dressingID != nil)
    }

    @discardableResult
    private func answer(_ decision: ClaudeHookMessage.Decision) -> Bool {
        guard let target = keyTarget(decision) else { return false }
        ClaudeSessionService.shared.answerKey(decision, target: target)
        return true
    }

    private func scheduleKeysSync() {
        guard keysWork == nil else { return }
        let work = DispatchWorkItem { [weak self] in
            self?.keysWork = nil
            self?.syncKeys()
        }
        keysWork = work
        DispatchQueue.main.async(execute: work)
    }

    /// With the keyboard, the panel reads ⌘Y, ⌘N and ⌘T itself. Opened by
    /// itself without it, they are borrowed from the app in front only while
    /// a request they answer is on screen, as the island does. Hold to talk
    /// is the menu bar's while the island does not show the sessions.
    private func syncKeys() {
        let defaults = UserDefaults.standard
        let islandHolds = NotchService.shared.running && NotchSupport.modules().contains(.sessions)
        let talks = followsSessions && !islandHolds
            && defaults.bool(forKey: DefaultsKey.notchSessionsVoiceReplies)
            && defaults.bool(forKey: DefaultsKey.notchSessionsVoiceShortcutEnabled)
        let voice = GlobalShortcut.saved(for: DefaultsKey.notchSessionsVoiceShortcut, fallback: .sessionVoiceDefault)
        voiceKey.sync(enabled: talks && !ShortcutCapture.isCapturing, shortcut: voice,
                      storageKey: DefaultsKey.notchSessionsVoiceShortcut)
        let borrows = followsSessions && panelShown && panel?.isKeyWindow != true && !ShortcutCapture.isCapturing
        allowKey.sync(enabled: borrows && keyTarget(.allow) != nil, shortcut: NotchClaudeSessionsSupport.allowShortcut,
                      storageKey: NotchClaudeSessionsSupport.allowShortcutKey)
        denyKey.sync(enabled: borrows && keyTarget(.deny) != nil, shortcut: NotchClaudeSessionsSupport.denyShortcut,
                     storageKey: NotchClaudeSessionsSupport.denyShortcutKey)
        alwaysKey.sync(enabled: borrows && keyTarget(.allowAlways) != nil,
                       shortcut: NotchClaudeSessionsSupport.alwaysShortcut,
                       storageKey: NotchClaudeSessionsSupport.alwaysShortcutKey)
    }
}

/// Brings the sessions up from outside their page — the hold-to-talk key,
/// the card on return — where they live: the island's page while the island
/// shows them and the panel is closed, else the menu bar's panel (#331).
enum SessionsSurface {
    /// The island's page or the panel shows the sessions now.
    static var isShown: Bool {
        let notch = NotchService.shared
        return SessionsMenuBar.shared.panelShown || (notch.expanded && notch.selected == .sessions)
    }

    /// The session whose page is on screen in either place.
    static var showingSession: String? { isShown ? ClaudeSessionService.shared.focusedID : nil }

    static func open(takeFocus: Bool = true) {
        let menuBar = SessionsMenuBar.shared
        if menuBar.panelShown || (!NotchService.shared.presentsSessions && menuBar.followsSessions) {
            menuBar.openPanel(makeKey: takeFocus)
        } else {
            NotchService.shared.open(.sessions, takeFocus: takeFocus)
        }
    }
}

/// A menu item's action, kept alive by the item that names it.
private final class SessionsMenuAction: NSObject {
    private let action: () -> Void
    init(_ action: @escaping () -> Void) { self.action = action }
    @objc func run() { action() }
}
