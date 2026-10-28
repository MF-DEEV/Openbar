// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import CoreGraphics
import Foundation

enum NotchClawdSpriteTests {
    static func run(_ suite: TestSuite) {
        suite.run("clawd drawing") { drawing(suite) }
        suite.run("codex robot drawing") { robot(suite) }
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
        suite.expect(everyCostume.count == 11 * 13 * 20 * 25, "a Clawd can be dressed \(everyCostume.count) ways")
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
            let covers = eyewear == .shades || eyewear == .anaglyph || eyewear.rawValue.hasSuffix("Mask")
            suite.expect(covers ? !eyes.contains("E") : eyes.allSatisfy { $0 == "E" },
                         "\(eyewear) \(covers ? "covers" : "shows") the eyes: \(eyes)")
        }
        // Windows draws from its own copy of these drawings (sprite.ts); the
        // same fingerprint on both keeps the copies alike pixel for pixel.
        var fingerprint: UInt32 = 0x811c_9dc5
        for character in [ClawdCharacter.clawd, .codex] {
            let classic = ClawdCostume.classic
            let parts = ClawdHat.allCases.map { ClawdCostume(shell: classic.shell, hat: $0, eyewear: .none, extra: .none) }
                + ClawdEyewear.allCases.map { ClawdCostume(shell: classic.shell, hat: .none, eyewear: $0, extra: .none) }
                + ClawdExtra.allCases.map { ClawdCostume(shell: classic.shell, hat: .none, eyewear: .none, extra: $0) }
            for costume in parts {
                for letter in NotchClawdSprite.letters(costume, NotchClawdPose(), character: character) {
                    fingerprint = (fingerprint ^ UInt32((letter ?? ".").asciiValue ?? 0)) &* 0x0100_0193
                }
            }
        }
        suite.expect(fingerprint == 0x54f1_eaf2, "every part is drawn as on Windows: \(String(fingerprint, radix: 16))")
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

    /// The same costumes the drawing test dresses Clawd in, which stand for
    /// every costume.
    static var standingCostumes: [ClawdCostume] {
        var costumes = ClawdShell.allCases.map { ClawdCostume(shell: $0, hat: .party, eyewear: .anaglyph, extra: .coffee) }
        costumes += ClawdHat.allCases.map { ClawdCostume(shell: .iris, hat: $0, eyewear: .none, extra: .headphones) }
        costumes += ClawdEyewear.allCases.map { ClawdCostume(shell: .iris, hat: .topHat, eyewear: $0, extra: .none) }
        costumes += ClawdExtra.allCases.map { ClawdCostume(shell: .iris, hat: .chef, eyewear: .none, extra: $0) }
        return costumes + [ClawdCostume(shell: .iris, hat: .none, eyewear: .none, extra: .none)]
    }

    static func robot(_ suite: TestSuite) {
        var clipped: [String] = [], unpainted: Set<Character> = []
        for costume in standingCostumes {
            for pose in reachingPoses {
                if NotchClawdSprite.clippedPixels(costume, pose, character: .codex) > 0 {
                    clipped.append("\(costume.code) \(pose.key)")
                }
                for letter in NotchClawdSprite.letters(costume, pose, character: .codex).compactMap({ $0 })
                where NotchClawdSprite.color(letter, costume: costume, character: .codex) == nil { unpainted.insert(letter) }
            }
        }
        suite.expect(clipped.isEmpty, "Codex's robot fits the canvas in every costume and pose: \(clipped.prefix(5))")
        suite.expect(unpainted.isEmpty, "every letter of the robot has a color: \(unpainted)")

        let width = NotchClawdSprite.width, x = NotchClawdSprite.bodyX, y = NotchClawdSprite.bodyY
        let bare = ClawdCostume(shell: .iris, hat: .none, eyewear: .none, extra: .none)
        func letter(_ costume: ClawdCostume, _ character: ClawdCharacter, column: Int, row: Int,
                    pose: NotchClawdPose = NotchClawdPose()) -> Character? {
            NotchClawdSprite.letters(costume, pose, character: character)[(y + row) * width + x + column]
        }
        suite.expect(letter(bare, .codex, column: 0, row: 0) == nil && letter(bare, .codex, column: 1, row: 0) == "C"
                     && letter(bare, .codex, column: 5, row: 1) == "V" && letter(bare, .codex, column: 9, row: 6) == "G",
                     "the robot is a terminal with rounded corners, a dark screen and a status light")
        suite.expect(letter(bare, .codex, column: 3, row: 2) == "E" && letter(bare, .codex, column: 9, row: 3) == "E",
                     "its eyes sit on the screen where Clawd's sit, so every pair of glasses fits")
        let glow = NotchClawdSprite.color("E", costume: bare, character: .codex)
        let screen = NotchClawdSprite.color("V", costume: bare, character: .codex)
        suite.expect(glow.map { "\($0)" } == "\(NotchClawdSprite.codexGlow)" && screen.map { "\($0)" } == "\(NotchClawdSprite.codexScreen)"
                     && NotchClawdSprite.color("E", costume: bare).map { "\($0)" } == "(red: 24, green: 22, blue: 26)",
                     "the robot's eyes glow on its screen, and Clawd's stay dark")
        suite.expect(letter(bare, .codex, column: 6, row: -2) == "G"
                     && letter(ClawdCostume(shell: .iris, hat: .topHat, eyewear: .none, extra: .none), .codex,
                               column: 6, row: -2) != "G",
                     "an antenna stands on its head, and a hat covers it")
        suite.expect(letter(bare, .codex, column: 3, row: 9) == "c" && letter(bare, .codex, column: 9, row: 9) == "c"
                     && letter(bare, .codex, column: 0, row: 9) == nil && letter(bare, .clawd, column: 0, row: 9) == "c",
                     "it stands on two stubby feet, not Clawd's four legs")
        suite.expect(NotchClawdSprite.letters(bare, NotchClawdPose()) == NotchClawdSprite.letters(bare, NotchClawdPose(), character: .clawd)
                     && NotchClawdSprite.letters(bare, NotchClawdPose(), character: .codex) != NotchClawdSprite.letters(bare, NotchClawdPose()),
                     "Clawd is drawn unless the robot is asked for")
        let typing = Set(stride(from: 0.0, to: 4, by: 1 / 12).map {
            NotchClawdSprite.letters(bare, NotchClawdSprite.pose(.think, at: $0, seed: 1), character: .codex)
        })
        suite.expect(typing.count > 1, "the robot's hands move as it works")
        let coffee = ClawdCostume(shell: .iris, hat: .none, eyewear: .none, extra: .coffee)
        suite.expect(letter(coffee, .codex, column: 14, row: 3) == "N" && letter(coffee, .clawd, column: 15, row: 3) == "N",
                     "a cup sits in the robot's hand, a pixel closer than in Clawd's claw")
        let robot = NotchClawdSprite.image(bare, NotchClawdPose(), character: .codex)
        suite.expect(robot != nil && robot !== NotchClawdSprite.image(bare, NotchClawdPose())
                     && NotchClawdSprite.image(bare, NotchClawdPose(), character: .codex) === robot,
                     "the robot's image is made and kept apart from Clawd's in the same costume")
        let bytes = NotchClawdSprite.pixels(bare, NotchClawdPose(), character: .codex)
        suite.expect(stride(from: 3, to: bytes.count, by: 4).allSatisfy { bytes[$0] == 0 || bytes[$0] == 255 },
                     "every pixel of the robot is opaque or clear")
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
