// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit

/// The sessions' own menu bar item (#331): the agent's face, a dot while a
/// session waits on the person and how many wait from two on, pulsing when
/// an ask arrives. A click opens the panel; a right click lists the asks.
final class SessionsStatusItem: NSObject {
    var onClick: (() -> Void)?
    var onRightClick: (() -> Void)?

    private let item: NSStatusItem
    private var waiting = 0
    private var pulseTimer: Timer?
    private var pulseStart = Date.distantPast
    private static let autosaveName = "OpenbarSessions"

    override init() {
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()
        // A stable identity, so macOS keeps the place the person gave it.
        item.autosaveName = Self.autosaveName
        item.behavior = []
        item.isVisible = true
        if let button = item.button {
            button.image = SessionsStatusIcon.image(waiting: false)
            button.imagePosition = .imageOnly
            button.font = MenuBarRenderer.statusFont(stacked: false)
            button.target = self
            button.action = #selector(clicked)
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
        }
    }

    func remove() {
        pulseTimer?.invalidate()
        pulseTimer = nil
        NSStatusBar.system.removeStatusItem(item)
    }

    /// The face with its dot and count, and what VoiceOver and the tooltip say.
    func update(waiting: Int, text: NotchClaudeSessionStrings) {
        guard let button = item.button else { return }
        let changed = waiting != self.waiting
        self.waiting = waiting
        if changed || button.image == nil {
            if waiting == 0 { stopPulse() }
            if pulseTimer == nil { button.image = SessionsStatusIcon.image(waiting: waiting > 0) }
        }
        let count = SessionsMenuBarSupport.countTitle(waiting)
        if button.title != count {
            button.title = count
            button.imagePosition = count.isEmpty ? .imageOnly : .imageLeading
        }
        let value = text.menuBarWaiting(waiting)
        button.setAccessibilityLabel(text.menuBarItemName)
        button.setAccessibilityValue(value)
        let tip = "\(text.menuBarItemName): \(value)"
        if button.toolTip != tip { button.toolTip = tip }
    }

    /// The dot breathes a few times, unless motion is reduced.
    func pulse() {
        guard waiting > 0, let button = item.button,
              !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else { return }
        pulseStart = Date()
        guard pulseTimer == nil else { return }
        let timer = Timer(timeInterval: 1.0 / 30, repeats: true) { [weak self, weak button] _ in
            guard let self, let button else { return }
            let elapsed = Date().timeIntervalSince(self.pulseStart)
            guard elapsed < SessionsStatusIcon.pulseDuration, self.waiting > 0 else {
                self.stopPulse()
                return
            }
            button.image = SessionsStatusIcon.image(waiting: true, swell: SessionsStatusIcon.swell(at: elapsed))
        }
        RunLoop.main.add(timer, forMode: .common)
        pulseTimer = timer
    }

    private func stopPulse() {
        pulseTimer?.invalidate()
        pulseTimer = nil
        item.button?.image = SessionsStatusIcon.image(waiting: waiting > 0)
    }

    /// Lit while the panel hangs from it, as a menu's title is.
    func setHighlighted(_ on: Bool) { item.button?.highlight(on) }

    /// The item's place on screen, when macOS gave it one that can be trusted.
    var trustedFrame: CGRect? {
        guard item.isVisible, let frame = item.button?.window?.frame,
              StatusItemAnchorSupport.isTrustworthyStatusFrame(frame) else { return nil }
        return frame
    }

    func contains(_ point: NSPoint) -> Bool {
        trustedFrame?.insetBy(dx: -4, dy: -8).contains(point) == true
    }

    /// Opens a menu from the item, as a menu bar item's own menu opens.
    func popUp(_ menu: NSMenu) {
        item.menu = menu
        item.button?.performClick(nil)
        DispatchQueue.main.async { [weak self] in self?.item.menu = nil }
    }

    @objc private func clicked() {
        if NSApp.currentEvent?.type == .rightMouseUp { onRightClick?() } else { onClick?() }
    }
}

/// The item's picture: Openbar's Sessions mark, an agent's face with
/// Clawd's two tall eyes (#249). Alone it is a template, so it follows the
/// menu bar's light and dark look; waiting, an orange dot sits on its corner
/// in a clear ring, and the face is drawn in the bar's own label color.
enum SessionsStatusIcon {
    static let size = NSSize(width: 23, height: 20)
    static let pulseDuration: TimeInterval = 1.8
    private static let pulsePeriod: TimeInterval = 0.6
    private static let dotRadius: CGFloat = 3.4
    private static let ringWidth: CGFloat = 1.3

    /// How much the dot has swelled, 0 to 1, at a moment of the pulse:
    /// three breaths, each out and back.
    static func swell(at elapsed: TimeInterval) -> CGFloat {
        guard elapsed >= 0, elapsed < pulseDuration else { return 0 }
        let phase = elapsed.truncatingRemainder(dividingBy: pulsePeriod) / pulsePeriod
        return CGFloat(sin(Double.pi * phase))
    }

    static func image(waiting: Bool, swell: CGFloat = 0) -> NSImage {
        let faceBox = NSRect(x: 1, y: 2.5, width: 20, height: 16.5)
        let image = NSImage(size: size, flipped: true) { _ in
            let face = NSBezierPath()
            face.windingRule = .evenOdd
            let parts = AgentFaceShape.parts(in: faceBox)
            for part in parts {
                face.append(NSBezierPath(roundedRect: part.rect, xRadius: part.radius, yRadius: part.radius))
            }
            (waiting ? NSColor.labelColor : NSColor.black).setFill()
            face.fill()
            guard waiting, let outline = parts.first?.rect else { return true }
            let center = NSPoint(x: outline.maxX - 0.9, y: outline.minY + 1.2)
            let radius = dotRadius * (1 + 0.35 * max(0, min(1, swell)))
            func circle(_ r: CGFloat) -> NSBezierPath {
                NSBezierPath(ovalIn: NSRect(x: center.x - r, y: center.y - r, width: r * 2, height: r * 2))
            }
            // A clear ring parts the dot from the face behind it.
            if let context = NSGraphicsContext.current {
                context.saveGraphicsState()
                context.compositingOperation = .clear
                circle(radius + ringWidth).fill()
                context.restoreGraphicsState()
            }
            NSColor.systemOrange.setFill()
            circle(radius).fill()
            return true
        }
        image.isTemplate = !waiting
        return image
    }
}
