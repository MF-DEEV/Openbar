// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import QuartzCore

/// A Clawd on screen: one layer showing the image of its pose, scaled up a
/// whole number of device pixels per canvas pixel with no smoothing, so its
/// edges stay sharp. A clock steps it a dozen times a second while it can be
/// seen, and the layer changes only when the pose does. Settings keeps its
/// views when it closes, so the window's visibility is watched as well.
final class NotchClawdView: NSView {
    private let sprite = CALayer()
    private(set) var costume = ClawdCostume.classic
    private(set) var motion = NotchClawdMotion.rest
    private var seed: UInt64 = 0
    private var still = false
    private var bubbles = true
    /// When the hop for finished work began, on the media clock.
    private var cheerStart: CFTimeInterval?
    private var cheerSince: Date?
    private var since: Date?
    private var powerObserver: NSObjectProtocol?
    private var configured = false
    private(set) var pose: NotchClawdPose?
    private var visibilityObserver: NSObjectProtocol?
    /// When the motion under way began, on the media clock. Updates that
    /// change nothing keep it, so the motion never jumps back.
    private(set) var motionStart: CFTimeInterval = 0
    private var clock: NotchDecorativeClock?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.masksToBounds = false
        sprite.magnificationFilter = .nearest
        sprite.minificationFilter = .nearest
        sprite.contentsGravity = .resize
        sprite.actions = ["contents": NSNull(), "bounds": NSNull(), "position": NSNull(), "frame": NSNull(),
                          "contentsScale": NSNull()]
        layer?.addSublayer(sprite)
        setAccessibilityElement(false)
        // Low Power Mode halves the steps from the moment it changes.
        powerObserver = NotificationCenter.default.addObserver(forName: .NSProcessInfoPowerStateDidChange, object: nil,
                                                               queue: .main) { [weak self] _ in self?.updateLayers() }
    }

    required init?(coder: NSCoder) { nil }

    deinit {
        if let visibilityObserver { NotificationCenter.default.removeObserver(visibilityObserver) }
        if let powerObserver { NotificationCenter.default.removeObserver(powerObserver) }
    }

    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// Shows `costume` doing `motion`. `seed` is the Clawd's own rhythm.
    /// `since` is when what it does began, so every Clawd of one session
    /// moves as one and a request's first hop comes as it arrives. `cheer`
    /// is when the work it hops for ended. `still` is Reduce Motion.
    func configure(costume: ClawdCostume, motion: NotchClawdMotion, seed: UInt64, since: Date? = nil,
                   cheer: Date? = nil, still: Bool, bubbles: Bool = true) {
        let restart = !configured || motion != self.motion || since != self.since
        guard restart || costume != self.costume || seed != self.seed || still != self.still || cheer != cheerSince
                || bubbles != self.bubbles
        else { return }
        configured = true
        self.costume = costume
        self.motion = motion
        self.seed = seed
        self.still = still
        self.bubbles = bubbles
        self.since = since
        let now = CACurrentMediaTime()
        if restart { motionStart = now - max(0, since.map { Date().timeIntervalSince($0) } ?? 0) }
        if cheer != cheerSince {
            cheerSince = cheer
            cheerStart = cheer.map { now - Date().timeIntervalSince($0) }
        }
        pose = nil
        updateLayers()
    }

    func stop() {
        still = true
        updateLayers()
        if let visibilityObserver { NotificationCenter.default.removeObserver(visibilityObserver) }
        visibilityObserver = nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let visibilityObserver { NotificationCenter.default.removeObserver(visibilityObserver) }
        visibilityObserver = window.map { window in
            NotificationCenter.default.addObserver(forName: NSWindow.didChangeOcclusionStateNotification,
                                                   object: window, queue: .main) { [weak self] _ in
                self?.updateLayers()
            }
        }
        updateLayers()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        placeSprite()
    }

    override func viewDidHide() { super.viewDidHide(); updateLayers() }
    override func viewDidUnhide() { super.viewDidUnhide(); updateLayers() }
    override func layout() { super.layout(); placeSprite() }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        placeSprite()
    }

    var isMoving: Bool { clock?.isRunning == true }

    private var backingScale: CGFloat { window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2 }

    /// The canvas fitted to the view, on whole device pixels.
    private func placeSprite() {
        let fit = NotchClawdSprite.layout(box: bounds.size, scale: backingScale)
        let frame = CGRect(x: fit.origin.x, y: fit.origin.y,
                           width: CGFloat(NotchClawdSprite.width) * fit.pixel,
                           height: CGFloat(NotchClawdSprite.height) * fit.pixel)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if sprite.frame != frame { sprite.frame = frame }
        sprite.contentsScale = backingScale
        CATransaction.commit()
    }

    private func updateLayers() {
        let moving = !still && configured && !isHiddenOrHasHiddenAncestor
            && window?.isVisible == true && window?.occlusionState.contains(.visible) == true
        placeSprite()
        show(at: CACurrentMediaTime())
        if moving {
            // Low Power Mode halves the steps; the poses keep their timing.
            let rate = ProcessInfo.processInfo.isLowPowerModeEnabled ? NotchClawdSprite.rate / 2 : NotchClawdSprite.rate
            if clock?.stepsPerSecond != rate {
                clock?.stop()
                clock = NotchDecorativeClock(rate: rate) { [weak self] time in self?.step(at: time) }
            }
            if let clock, let window, !clock.isRunning, window.contentView != nil { clock.start(in: self) }
        } else {
            clock?.stop()
        }
    }

    /// The pose for `time` on the media clock, set on the layer when it
    /// differs from the one shown.
    private func show(at time: CFTimeInterval) {
        guard configured else { return }
        let next = NotchClawdSprite.pose(motion, at: time - motionStart, seed: seed,
                                         cheer: cheerStart.map { time - $0 }, still: still, bubbles: bubbles)
        guard next != pose else { return }
        pose = next
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        sprite.contents = NotchClawdSprite.image(costume, next)
        CATransaction.commit()
    }

    func step(at time: CFTimeInterval) {
        guard isMoving else { return }
        show(at: time)
    }
}
