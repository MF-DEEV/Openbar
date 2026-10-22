// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import QuartzCore

enum NotchClawdViewTests {
    private final class Window: NSWindow {
        var shown = true
        var exposed = true
        override var isVisible: Bool { shown }
        override var occlusionState: NSWindow.OcclusionState { exposed ? [.visible] : [] }
        func notifyVisibility() {
            NotificationCenter.default.post(name: NSWindow.didChangeOcclusionStateNotification, object: self)
        }
    }

    static func run(_ suite: TestSuite) {
        suite.run("clawd view") { view(suite) }
    }

    static func view(_ suite: TestSuite) {
        let window = Window(contentRect: CGRect(x: 0, y: 0, width: 80, height: 80),
                            styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let container = NSView(frame: window.frame)
        window.contentView = container
        let costume = ClawdCostume(shell: .sky, hat: .cap, eyewear: .none, extra: .none)
        let view = NotchClawdView(frame: CGRect(x: 0, y: 0, width: 32, height: 30))
        func animationCount(_ layer: CALayer? = nil) -> Int {
            guard let layer = layer ?? view.layer else { return 0 }
            return (layer.animationKeys() ?? []).count + (layer.sublayers ?? []).reduce(0) { $0 + animationCount($1) }
        }
        func sprite() -> CALayer? { view.layer?.sublayers?.first }

        view.configure(costume: costume, motion: .work, seed: 7, still: false)
        suite.expect(!view.isMoving, "a Clawd not on screen does not move")
        container.addSubview(view)
        suite.expect(view.isMoving, "on screen it moves")
        suite.expect(animationCount() == 0, "it leaves no repeating animation for the window server to draw at the display's rate")
        suite.expect(sprite()?.contents != nil && sprite()?.magnificationFilter == .nearest
                     && sprite()?.minificationFilter == .nearest, "its image is scaled without smoothing")
        let frame = sprite()?.frame ?? .zero
        let scale = window.backingScaleFactor
        suite.expect(abs(frame.width * scale - (frame.width * scale).rounded()) < 0.001
                     && abs(frame.width / CGFloat(NotchClawdSprite.width) * scale
                            - (frame.width / CGFloat(NotchClawdSprite.width) * scale).rounded()) < 0.001,
                     "each canvas pixel covers whole device pixels: \(frame)")

        let started = view.motionStart
        view.configure(costume: costume, motion: .work, seed: 7, still: false)
        suite.expect(view.motionStart == started, "an update that changes nothing keeps the motion where it is")
        let poses = stride(from: 0.0, to: 2, by: 1 / 12).map { time -> NotchClawdPose? in
            view.step(at: started + time)
            return view.pose
        }
        suite.expect(Set(poses.compactMap { $0 }).count > 1, "stepping the clock moves it from pose to pose")
        view.step(at: started + 0.5)
        let shown = sprite()?.contents as AnyObject?
        view.step(at: started + 0.5)
        suite.expect(sprite()?.contents as AnyObject? === shown, "the same pose keeps the same image")

        view.configure(costume: costume, motion: .alert, seed: 7, still: false)
        suite.expect(view.motionStart > started, "a new motion starts from its beginning")
        view.configure(costume: costume, motion: .alert, seed: 7, still: true)
        suite.expect(!view.isMoving && view.pose?.bubble == .alert, "Reduce Motion stops it in a pose that says the same")
        view.configure(costume: costume, motion: .alert, seed: 7, still: false)
        view.configure(costume: costume, motion: .alert, seed: 7, still: false, bubbles: false)
        suite.expect(view.pose?.bubble == nil, "a Clawd in a notice shows no bubble")
        view.configure(costume: costume, motion: .content, seed: 7, cheer: Date().addingTimeInterval(-0.3), still: false)
        view.step(at: CACurrentMediaTime())
        suite.expect((view.pose?.jump ?? 0) > 0 || view.pose?.arms == .up, "work that just ended makes it jump")
        view.configure(costume: costume, motion: .content, seed: 7, cheer: Date().addingTimeInterval(-60), still: false)
        view.step(at: CACurrentMediaTime())
        suite.expect(view.pose?.jump == 0, "work that ended long ago does not")
        let resting = view.motionStart
        view.configure(costume: costume, motion: .content, seed: 7, cheer: Date().addingTimeInterval(-0.3), still: false)
        view.step(at: CACurrentMediaTime())
        suite.expect(((view.pose?.jump ?? 0) > 0 || view.pose?.arms == .up) && view.motionStart == resting,
                     "a new change in the wardrobe makes it hop again, with no new start for what it does")
        view.configure(costume: costume, motion: .content, seed: 7, cheer: Date().addingTimeInterval(-0.3), still: true)
        view.configure(costume: costume, motion: .content, seed: 7, cheer: Date().addingTimeInterval(-0.3), still: false)
        suite.expect(view.motionStart == resting, "turning Reduce Motion off again goes on from where it was")

        // A subagent that finished holds one pose, so only the new costume
        // can change what it shows.
        view.configure(costume: costume, motion: .done, seed: 7, still: false)
        let held = view.pose
        let before = sprite()?.contents as AnyObject?
        let dressed = ClawdCostume(shell: .rose, hat: .crown, eyewear: .round, extra: .bow)
        view.configure(costume: dressed, motion: .done, seed: 7, still: false)
        let after = sprite()?.contents as AnyObject?
        suite.expect(view.pose == held && after !== before
                     && after === NotchClawdSprite.image(dressed, held ?? NotchClawdPose()) as AnyObject?,
                     "a new costume shows at once, even in a pose that does not change")

        // The canvas is placed from the top: its feet stand on the frame's
        // foot and a hat may reach above the box, never the other way round.
        for size in [CGSize(width: 32, height: 30), CGSize(width: 48, height: 42)] {
            view.setFrameSize(size)
            let fit = NotchClawdSprite.layout(box: size, scale: window.backingScaleFactor)
            let placed = sprite()?.frame ?? .zero
            suite.expect(view.layer?.isGeometryFlipped == true
                         && placed == CGRect(x: fit.origin.x, y: fit.origin.y,
                                             width: CGFloat(NotchClawdSprite.width) * fit.pixel,
                                             height: CGFloat(NotchClawdSprite.height) * fit.pixel),
                         "in a \(size) box the canvas sits where the fitting puts it, measured from the top: \(placed)")
        }
        view.setFrameSize(CGSize(width: 32, height: 30))

        window.exposed = false
        window.notifyVisibility()
        suite.expect(!view.isMoving, "a covered island stops it")
        window.exposed = true
        window.notifyVisibility()
        suite.expect(view.isMoving, "and uncovering it starts it again")
        window.shown = false
        window.notifyVisibility()
        suite.expect(!view.isMoving, "closed Settings keep it still")
        window.shown = true
        window.notifyVisibility()
        container.isHidden = true
        suite.expect(!view.isMoving, "a hidden page stops it")
        container.isHidden = false
        suite.expect(view.isMoving, "and showing the page starts it")
        suite.expect(view.hitTest(.zero) == nil && !view.isAccessibilityElement(),
                     "a Clawd takes no clicks and is not read out on its own")
        view.removeFromSuperview()
        suite.expect(!view.isMoving, "taking it off screen stops it")
        container.addSubview(view)
        view.stop()
        window.notifyVisibility()
        suite.expect(!view.isMoving, "and a dismantled Clawd stays still")
        view.removeFromSuperview()

        let clock = NotchDecorativeClock(rate: NotchClawdSprite.rate) { _ in }
        let host = NSView(frame: container.bounds)
        container.addSubview(host)
        clock.start(in: host)
        let range = clock.displayLink?.preferredFrameRateRange
        suite.expect(range?.maximum == NotchClawdSprite.rate && NotchClawdSprite.rate <= 12,
                     "a Clawd steps at most a dozen times a second: \(String(describing: range))")
        clock.stop()
        suite.expect(NotchDecorativeClock(rate: 120) { _ in }.stepsPerSecond == NotchDecorativeClock.rate
                     && NotchDecorativeClock { _ in }.stepsPerSecond == NotchDecorativeClock.rate,
                     "no clock steps faster than the decorations' own rate")

        let row = NotchClawdView(frame: CGRect(x: 0, y: 0, width: 32, height: 30))
        let notice = NotchClawdView(frame: CGRect(x: 0, y: 0, width: 18, height: 18))
        let asked = Date().addingTimeInterval(-0.9)
        row.configure(costume: costume, motion: .alert, seed: 3, since: asked, still: false)
        usleep(150_000)
        notice.configure(costume: costume, motion: .alert, seed: 3, since: asked, still: false, bubbles: false)
        suite.expect(abs(row.motionStart - notice.motionStart) < 0.05,
                     "two Clawds of one session, drawn at different times, hop as one")
        suite.expect(abs((CACurrentMediaTime() - notice.motionStart) - Date().timeIntervalSince(asked)) < 0.05,
                     "their motion counts from when the session began waiting, so the first hop comes as the request does")
        let rowStart = row.motionStart
        row.configure(costume: costume, motion: .alert, seed: 3, since: asked, still: false)
        suite.expect(row.motionStart == rowStart, "the same moment given again changes nothing")
        row.configure(costume: costume, motion: .alert, seed: 3, since: Date(), still: false)
        suite.expect(row.motionStart > rowStart, "a new request starts the hops again")

        weak var released: NotchClawdView?
        autoreleasepool {
            let view = NotchClawdView(frame: .zero)
            released = view
            view.configure(costume: costume, motion: .think, seed: 1, still: false)
            container.addSubview(view)
            view.removeFromSuperview()
        }
        suite.expect(released == nil, "watching the window does not keep a discarded Clawd alive")
    }
}
