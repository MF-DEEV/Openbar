// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import SwiftUI

/// The panel under the sessions' menu bar item (#331). It takes the
/// keyboard without bringing Openbar forward, as the island does, so a
/// reply can be typed while the app in front keeps its menus.
final class SessionsMenuBarPanel: OverlayPanel {
    /// Keys the panel reads before its page: Escape, ⌘W, ⌘Y, ⌘N and ⌘T.
    var onKey: ((NSEvent) -> Bool)?
    /// The panel took or gave up the keyboard.
    var onKeyChange: (() -> Void)?

    init() {
        super.init(contentRect: NSRect(origin: .zero, size: SessionsMenuBarSupport.panelSize),
                   styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        isFloatingPanel = true
        // Above other windows, below menus, like the menu bar it hangs from.
        level = .statusBar
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle, .transient]
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        animationBehavior = .utilityWindow
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    /// Nothing while the panel is closed, so the page lets go of its
    /// session as the island's does and nothing ticks unseen.
    func clearContent() { contentView = nil }

    func setContent<Content: View>(_ view: Content) {
        let host = NSHostingView(rootView: view)
        host.wantsLayer = true
        host.layer?.backgroundColor = .clear
        contentView = host
        // The shadow follows the rounded page once it is drawn.
        DispatchQueue.main.async { [weak self] in self?.invalidateShadow() }
    }

    override func setFrame(_ frameRect: NSRect, display flag: Bool) {
        super.setFrame(frameRect, display: flag)
        invalidateShadow()
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, onKey?(event) == true { return }
        super.sendEvent(event)
    }

    override func becomeKey() {
        super.becomeKey()
        onKeyChange?()
    }

    override func resignKey() {
        super.resignKey()
        onKeyChange?()
    }
}

/// The island's Sessions page in the panel: the same page and top row as
/// the open island, on the same black surface, with rounded corners.
struct SessionsMenuBarPanelView: View {
    @ObservedObject private var menuBar = SessionsMenuBar.shared
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        let size = menuBar.panelSize
        let content = SessionsMenuBarSupport.contentSize(in: size)
        var band = NotchBand()
        band.side = content.width
        let shape = RoundedRectangle(cornerRadius: SessionsMenuBarSupport.cornerRadius, style: .continuous)
        return NotchClaudeSessionsView(size: content)
            .frame(width: content.width, height: content.height, alignment: .top)
            .clipped()
            .padding(.top, NotchLayout.headerHeight + NotchLayout.spacing)
            .overlayPreferenceValue(NotchBandKey.self, alignment: .top) { items in
                NotchBandRow(band: band) { items.leading } trailing: { items.trailing }
            }
            .environment(\.notchBand, band)
            .padding(.horizontal, SessionsMenuBarSupport.horizontalInset)
            .padding(.top, SessionsMenuBarSupport.headerTopInset)
            .padding(.bottom, NotchLayout.bottomInset)
            .frame(width: size.width, height: size.height, alignment: .top)
            .background(shape.fill(.black))
            .overlay(shape.strokeBorder(.white.opacity(0.08), lineWidth: 1))
            .clipShape(shape)
            .contentShape(shape)
            .foregroundStyle(.white)
            .tint(.white)
            .environment(\.colorScheme, .dark)
            .environment(\.notchPresentation, true)
            .environment(\.notchSessionsInPanel, true)
            .environment(\.locale, l10n.language.formattingLocale())
            .focusEffectDisabled(!menuBar.showsKeyboardFocus)
            .onPreferenceChange(NotchSessionsListHeightKey.self) { height in
                // Not while SwiftUI lays the panel out: the panel's frame follows.
                DispatchQueue.main.async { SessionsMenuBar.shared.fitList(height) }
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel(FeatureStrings.notchClaudeSessions(l10n.language).menuBarItemName)
    }
}
