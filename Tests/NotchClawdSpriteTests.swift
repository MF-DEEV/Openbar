// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import CoreGraphics
import Foundation

enum NotchClawdSpriteTests {
    static func run(_ suite: TestSuite) {
        suite.run("clawd drawing") { drawing(suite) }
        suite.run("clawd poses") { poses(suite) }
        suite.run("clawd fitting") { fitting(suite) }
    }

    static var everyCostume: [ClawdCostume] {
        ClawdShell.allCases.flatMap { shell in
            ClawdHat.allCases.flatMap { hat in
                ClawdEyewear.allCases.flatMap { eyewear in
                    ClawdExtra.allCases.map { ClawdCostume(shell: shell, hat: hat, eyewear: eyewear, extra: $0) }
                }
            }
        }
    }

    static let motions: [NotchClawdMotion] = [.rest, .content, .doze, .think, .work, .slow, .alert, .ask, .done]

    /// Every pose a Clawd is ever drawn in: each motion over ten seconds,
    /// the jump for finished work, and the still poses of Reduce Motion.
    static var reachingPoses: [NotchClawdPose] {
        var poses = Set<NotchClawdPose>()
        for motion in motions {
            for step in 0..<120 {
                let time = Double(step) / 12
                poses.insert(NotchClawdSprite.pose(motion, at: time, seed: 7))
                poses.insert(NotchClawdSprite.pose(motion, at: time, seed: 7, cheer: time))
                poses.insert(NotchClawdSprite.pose(motion, at: time, seed: 7, still: true))
            }
        }
        return Array(poses)
    }

    static func drawing(_ suite: TestSuite) {
        suite.expect(everyCostume.count == 10 * 9 * 6 * 7, "a Clawd can be dressed \(everyCostume.count) ways")
        // Each part is drawn on its own, so each part checked on its own
        // stands for every costume it is part of; the tallest hat with the
        // widest extra checks that they fit together too.
        let bare = ClawdCostume.classic
        var costumes = ClawdShell.allCases.map { ClawdCostume(shell: $0, hat: .party, eyewear: .anaglyph, extra: .coffee) }
        costumes += ClawdHat.allCases.map { ClawdCostume(shell: .clay, hat: $0, eyewear: .none, extra: .headphones) }
        costumes += ClawdEyewear.allCases.map { ClawdCostume(shell: .clay, hat: .topHat, eyewear: $0, extra: .none) }
        costumes += ClawdExtra.allCases.map { ClawdCostume(shell: .clay, hat: .chef, eyewear: .none, extra: $0) }
        var clipped: [String] = [], unpainted: Set<Character> = []
        for costume in costumes + [bare] {
            for pose in reachingPoses {
                if NotchClawdSprite.clippedPixels(costume, pose) > 0 { clipped.append("\(costume.code) \(pose.key)") }
                for letter in NotchClawdSprite.letters(costume, pose).compactMap({ $0 })
                where NotchClawdSprite.color(letter, costume: costume) == nil { unpainted.insert(letter) }
            }
        }
        suite.expect(clipped.isEmpty, "every costume fits the canvas in every pose: \(clipped.prefix(5))")
        suite.expect(unpainted.isEmpty, "every letter drawn has a color: \(unpainted)")

        let width = NotchClawdSprite.width, x = NotchClawdSprite.bodyX, y = NotchClawdSprite.bodyY
        func letter(_ costume: ClawdCostume, column: Int, row: Int) -> Character? {
            NotchClawdSprite.letters(costume, NotchClawdPose())[(y + row) * width + x + column]
        }
        suite.expect(letter(bare, column: 3, row: 2) == "E" && letter(bare, column: 9, row: 3) == "E"
                     && letter(bare, column: 6, row: 0) == "C" && letter(bare, column: 0, row: 9) == "c",
                     "Clawd has two eyes, a body and legs, in the shell's shade, where Claude Code draws them")
        for eyewear in ClawdEyewear.allCases {
            let worn = ClawdCostume(shell: .clay, hat: .none, eyewear: eyewear, extra: .none)
            let eyes = [(3, 2), (3, 3), (9, 2), (9, 3)].map { letter(worn, column: $0.0, row: $0.1) }
            let covers = eyewear == .shades || eyewear == .anaglyph
            suite.expect(covers ? !eyes.contains("E") : eyes.allSatisfy { $0 == "E" },
                         "\(eyewear) \(covers ? "covers" : "shows") the eyes: \(eyes)")
        }
        for shell in ClawdShell.allCases {
            suite.expect(NotchClawdSprite.accent(for: shell) != shell, "a hat on \(shell) is told from the shell")
        }
        suite.expect(Set(ClawdShell.allCases.map { "\($0.rgb)" }).count == ClawdShell.allCases.count,
                     "every shell has its own color")
        suite.expect(ClawdShell.clay.rgb == (215, 119, 87), "the classic shell is Claude Code's own clawd_body")

        let image = NotchClawdSprite.image(bare, NotchClawdPose())
        suite.expect(image?.width == width && image?.height == NotchClawdSprite.height,
                     "an image has a pixel per canvas pixel")
        suite.expect(NotchClawdSprite.image(bare, NotchClawdPose()) === image, "an image is made once per costume and pose")
        let bytes = NotchClawdSprite.pixels(bare, NotchClawdPose())
        suite.expect(stride(from: 3, to: bytes.count, by: 4).allSatisfy { bytes[$0] == 0 || bytes[$0] == 255 },
                     "every pixel is opaque or clear, so scaling without smoothing keeps its edges")
        var propeller = ClawdCostume.classic
        propeller.hat = .propeller
        suite.expect(NotchClawdSprite.letters(propeller, NotchClawdPose(tick: 0))
                     != NotchClawdSprite.letters(propeller, NotchClawdPose(tick: 1)),
                     "a propeller turns from one step to the next")
    }

    static func poses(_ suite: TestSuite) {
        let seed = ClawdCostume.seed("session")
        func poses(_ motion: NotchClawdMotion, over seconds: Double = 8) -> [NotchClawdPose] {
            stride(from: 0.0, to: seconds, by: 1 / 12).map { NotchClawdSprite.pose(motion, at: $0, seed: seed) }
        }
        let rest = poses(.rest)
        suite.expect(rest.contains { $0.eyes == .closed } && rest.contains { $0.rise == 1 } && rest.contains { $0.rise == 0 },
                     "at rest it blinks and breathes")
        suite.expect(rest.filter { $0.eyes == .closed }.count <= 2 * Int(8 / 3.4 + 1),
                     "a blink lasts two steps")
        suite.expect(poses(.doze).allSatisfy { $0.eyes == .closed }
                     && Set(poses(.doze).compactMap(\.bubble)).count == 3, "dozing, its eyes stay shut and the z's rise")
        suite.expect(Set(poses(.think).map(\.arms)) == [.typeLeft, .typeRight], "thinking, its hands are busy")
        suite.expect(Set(poses(.work).map(\.legs)) == [.stand, .stepLeft, .stepRight], "at work it waddles")
        let alert = poses(.alert)
        suite.expect(alert.allSatisfy { $0.bubble == .alert } && alert.contains { $0.jump > 0 }
                     && alert.contains { $0.arms == .waveUp } && alert.allSatisfy { $0.jump == 0 || $0.legs == .stand },
                     "waiting on a permission, it hops and waves under a !")
        suite.expect(poses(.ask).allSatisfy { $0.bubble == .question }, "waiting on an answer, it shows a ?")
        suite.expect(poses(.content).allSatisfy { $0.eyes == .happy } && Set(poses(.done)).count == 1,
                     "done, it smiles, and a subagent that finished keeps still")
        let other = ClawdCostume.seed("another session")
        let blinks = stride(from: 0.0, to: 20, by: 1 / 12).map { NotchClawdSprite.blinks($0, seed: seed) }
        let otherBlinks = stride(from: 0.0, to: 20, by: 1 / 12).map { NotchClawdSprite.blinks($0, seed: other) }
        suite.expect(blinks != otherBlinks, "two Clawds side by side do not blink as one")

        for motion in [NotchClawdMotion.rest, .content, .doze, .think, .work, .slow, .alert, .ask, .done] {
            let still = stride(from: 0.0, to: 6, by: 0.25).map {
                NotchClawdSprite.pose(motion, at: $0, seed: seed, cheer: 0.5, still: true)
            }
            suite.expect(Set(still).count == 1 && still[0].jump == 0 && still[0].rise == 0,
                         "with Reduce Motion, \(motion) is one pose that does not move")
        }
        let still = { (motion: NotchClawdMotion) in NotchClawdSprite.pose(motion, at: 0, seed: seed, still: true) }
        suite.expect(still(.alert).bubble == .alert && still(.ask).bubble == .question && still(.doze).eyes == .closed
                     && still(.content).eyes == .happy && still(.think).arms == .typeLeft,
                     "and each still pose says what the motion says")

        let cheer = stride(from: 0.0, to: NotchClawdSprite.cheerLength, by: 1 / 12).map {
            NotchClawdSprite.pose(.content, at: 30, seed: seed, cheer: $0)
        }
        suite.expect(cheer.contains { $0.jump >= 4 && $0.arms == .up } && cheer.allSatisfy { $0.eyes == .happy },
                     "when its work ends it jumps with both arms up")
        suite.expect(NotchClawdSprite.pose(.content, at: 30, seed: seed, cheer: NotchClawdSprite.cheerLength)
                     == NotchClawdSprite.pose(.content, at: 30, seed: seed),
                     "and once the jump is over, a Clawd drawn later does not jump again")
        suite.expect(NotchClawdSprite.pose(.alert, at: 0.2, seed: seed, bubbles: false).bubble == nil
                     && NotchClawdSprite.pose(.doze, at: 1, seed: seed, bubbles: false).bubble == nil,
                     "where there is no room above, no bubble shows")
        suite.expect(NotchClawdSprite.pose(.rest, at: .nan, seed: seed) == NotchClawdSprite.pose(.rest, at: 0, seed: seed)
                     && NotchClawdSprite.pose(.rest, at: -5, seed: seed) == NotchClawdSprite.pose(.rest, at: 0, seed: seed),
                     "a time that makes no sense counts as the start")
        suite.expect(Set(rest.map(\.key)).count == Set(rest).count, "each pose has a key of its own")
    }

    static func fitting(_ suite: TestSuite) {
        for (box, scale) in [(CGSize(width: 32, height: 30), 2.0), (CGSize(width: 40, height: 36), 2.0),
                             (CGSize(width: 18, height: 18), 2.0), (CGSize(width: 32, height: 30), 1.0),
                             (CGSize(width: 48, height: 42), 3.0), (CGSize(width: 130, height: 110), 2.0)] {
            let fit = NotchClawdSprite.layout(box: box, scale: scale)
            let devicePixels = fit.pixel * scale
            let frameWidth = CGFloat(NotchClawdSprite.frameColumns.count) * fit.pixel
            let frameHeight = CGFloat(NotchClawdSprite.frameRows.count) * fit.pixel
            let frameLeft = fit.origin.x + CGFloat(NotchClawdSprite.frameColumns.lowerBound) * fit.pixel
            let frameTop = fit.origin.y + CGFloat(NotchClawdSprite.frameRows.lowerBound) * fit.pixel
            suite.expect(devicePixels == devicePixels.rounded() && devicePixels >= 1,
                         "\(box) at \(scale)x: a canvas pixel is a whole number of device pixels: \(devicePixels)")
            suite.expect(frameWidth <= box.width + 0.001 && frameHeight <= box.height + 0.001,
                         "\(box) at \(scale)x: the figure fits its box")
            suite.expect(abs(frameLeft + frameWidth / 2 - box.width / 2) <= 0.5 / scale + 0.001
                         && abs(frameTop + frameHeight / 2 - box.height / 2) <= 0.5 / scale + 0.001,
                         "\(box) at \(scale)x: the figure is centred")
            suite.expect((fit.origin.x * scale).rounded() == fit.origin.x * scale
                         && (fit.origin.y * scale).rounded() == fit.origin.y * scale,
                         "\(box) at \(scale)x: the canvas starts on a device pixel")
        }
        suite.expect(NotchClawdSprite.layout(box: CGSize(width: 32, height: 30), scale: 2).pixel == 1.5,
                     "a Clawd in a list row gets three device pixels a canvas pixel")
        suite.expect(NotchClawdSprite.layout(box: .zero, scale: 2).pixel == 0.5
                     && NotchClawdSprite.layout(box: CGSize(width: 40, height: 40), scale: .nan).pixel > 0,
                     "an empty box or a scale that makes no sense still draws something")
        let odd = NotchClawdSprite.layout(box: CGSize(width: CGFloat.nan, height: .infinity), scale: 2)
        suite.expect(odd.pixel == 0.5 && odd.origin.x.isFinite && odd.origin.y.isFinite,
                     "a box that makes no sense is drawn as an empty one, never at a position that is not a number")
        // Every costume is the same size: a hat reaches above the frame instead.
        suite.expect(NotchClawdSprite.frameColumns.count == 17 && NotchClawdSprite.frameRows.upperBound == NotchClawdSprite.height,
                     "the frame is the body with its arms, standing on the canvas's bottom")
    }
}
