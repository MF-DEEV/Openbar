// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import CoreGraphics
import Foundation

/// What a Clawd does for as long as it is shown.
enum NotchClawdMotion: Equatable {
    /// Awake and at ease: it breathes, blinks and looks about.
    case rest
    /// Its work is done: smiling eyes.
    case content
    /// Long quiet: eyes shut and a "z" rising now and then.
    case doze
    /// Claude writes: busy hands, eyes going to and fro.
    case think
    /// A tool runs: it waddles on the spot.
    case work
    /// Work in the background: busy hands, at half the pace.
    case slow
    /// It waits for a permission: hops, waves and shows "!".
    case alert
    /// It waits for an answer: the same, with "?".
    case ask
    /// A subagent that finished: still, with smiling eyes.
    case done
}

/// One drawing of a Clawd. Equal poses draw the same image, so they key the
/// cache, and a new image is set only when the pose changes.
struct NotchClawdPose: Hashable {
    enum Arms: String { case down, up, typeLeft, typeRight, waveUp, waveOut }
    enum Legs: String { case stand, stepLeft, stepRight }
    enum Eyes: String { case open, closed, happy, left, right }
    enum Bubble: Hashable { case alert, question, sleep(Int) }

    var arms = Arms.down
    var legs = Legs.stand
    var eyes = Eyes.open
    var bubble: Bubble?
    /// How far the body rises over its legs, which stretch to stay on the ground.
    var rise = 0
    /// How far it is off the ground, legs tucked.
    var jump = 0
    /// Every other step, for what turns or steams on its own.
    var tick = 0

    var key: String {
        let bubbleKey: String
        switch bubble {
        case nil: bubbleKey = "-"
        case .alert: bubbleKey = "!"
        case .question: bubbleKey = "?"
        case .sleep(let phase): bubbleKey = "z\(phase)"
        }
        return "\(arms.rawValue).\(legs.rawValue).\(eyes.rawValue).\(bubbleKey).\(rise).\(jump).\(tick)"
    }
}

/// Clawd drawn in pixels, after the sprite Claude Code cooks with: a body 13
/// pixels wide and 8 tall, eyes a pixel wide and two tall, arms that stick
/// out two pixels on each side and four short legs. Each costume part is a
/// small drawing laid over it. Codex's robot fills the same footprint, so
/// every part fits it unchanged. Everything here is plain data and pure
/// functions; the views only put the images on screen.
enum NotchClawdSprite {
    // MARK: Canvas

    static let width = 24
    static let height = 22
    /// Where the body's top left pixel sits on the canvas.
    static let bodyX = 5
    static let bodyY = 12
    /// The part of the canvas the figure is fitted by: the body with its
    /// arms and legs and a little room for a hat. It is the same for every
    /// costume, so every Clawd is drawn at the same size; tall hats, a raised
    /// arm and the bubbles reach past it.
    static let frameColumns = 3..<20
    static let frameRows = 8..<22

    /// Steps a second while it moves.
    static let rate: Float = 12

    // MARK: Fitting

    /// How big a canvas pixel is, in points, and where the canvas's top left
    /// corner goes in a box measured from its top left: as many whole device
    /// pixels per canvas pixel as fit the frame in `box`, at least one, with
    /// the frame centred and every edge on a device pixel.
    static func layout(box: CGSize, scale: CGFloat) -> (pixel: CGFloat, origin: CGPoint) {
        let scale = max(1, scale.isFinite ? scale : 1)
        let box = CGSize(width: box.width.isFinite ? max(0, box.width) : 0, height: box.height.isFinite ? max(0, box.height) : 0)
        let across = box.width * scale / CGFloat(frameColumns.count)
        let down = box.height * scale / CGFloat(frameRows.count)
        let devicePixels = max(1, floor(min(across, down)))
        let pixel = devicePixels / scale
        let frameMidX = CGFloat(frameColumns.lowerBound) + CGFloat(frameColumns.count) / 2
        let frameMidY = CGFloat(frameRows.lowerBound) + CGFloat(frameRows.count) / 2
        let x = (box.width / 2 - frameMidX * pixel) * scale
        let y = (box.height / 2 - frameMidY * pixel) * scale
        return (pixel, CGPoint(x: x.rounded() / scale, y: y.rounded() / scale))
    }

    // MARK: Motion

    /// How long the hop a Clawd gives at the end of its work lasts.
    static let cheerLength: TimeInterval = 1.4

    /// The pose `motion` has reached `elapsed` seconds in. `seed` sets the
    /// Clawd's own rhythm, so a row of them never blinks as one. `cheer` is
    /// how long ago its work ended, while the hop for it plays. `still` is
    /// Reduce Motion: one pose that says the same without moving. Without
    /// `bubbles`, as in a notice with no room above, no "!", "?" or "z" shows.
    static func pose(_ motion: NotchClawdMotion, at elapsed: TimeInterval, seed: UInt64,
                     cheer: TimeInterval? = nil, still: Bool = false, bubbles: Bool = true) -> NotchClawdPose {
        var pose = posed(motion, at: elapsed, seed: seed, cheer: cheer, still: still)
        if !bubbles { pose.bubble = nil }
        return pose
    }

    private static func posed(_ motion: NotchClawdMotion, at elapsed: TimeInterval, seed: UInt64,
                              cheer: TimeInterval?, still: Bool) -> NotchClawdPose {
        if still { return stillPose(motion) }
        let time = elapsed.isFinite ? max(0, elapsed) : 0
        if let cheer, cheer >= 0, cheer < cheerLength { return cheerPose(at: cheer) }
        let step = Int(time * Double(rate))
        var pose = NotchClawdPose()
        pose.tick = (step / 3) % 2
        switch motion {
        case .rest:
            pose.eyes = blinks(time, seed: seed) ? .closed : glance(time, seed: seed)
            pose.rise = breath(time, period: 2.4)
        case .content:
            pose.eyes = .happy
            pose.rise = breath(time, period: 2.4)
        case .doze:
            pose.eyes = .closed
            pose.rise = breath(time, period: 3.2)
            pose.bubble = .sleep(Int(time / 0.7) % 3)
        case .think:
            pose.arms = (step / 2) % 2 == 0 ? .typeLeft : .typeRight
            let look = Int(time / 0.8) % 4
            pose.eyes = blinks(time, seed: seed) ? .closed : look == 1 ? .left : look == 3 ? .right : .open
        case .slow:
            pose.arms = (step / 4) % 2 == 0 ? .typeLeft : .typeRight
            pose.eyes = blinks(time, seed: seed) ? .closed : .open
        case .work:
            switch (step / 2) % 4 {
            case 0: pose.legs = .stepLeft
            case 2: pose.legs = .stepRight
            default: pose.rise = 1
            }
            pose.eyes = blinks(time, seed: seed) ? .closed : .open
        case .alert, .ask:
            pose.bubble = motion == .alert ? .alert : .question
            let cycle = time.truncatingRemainder(dividingBy: 1.6)
            if cycle < 0.5 {
                pose.jump = [0, 2, 3, 3, 2, 0][min(5, Int(cycle * Double(rate)))]
                pose.arms = pose.jump > 0 ? .up : .down
            } else if cycle < 1.1 {
                pose.arms = Int((cycle - 0.5) / 0.15) % 2 == 0 ? .waveUp : .waveOut
            }
            pose.eyes = .open
        case .done:
            pose.eyes = .happy
            pose.tick = 0
        }
        return pose
    }

    private static func stillPose(_ motion: NotchClawdMotion) -> NotchClawdPose {
        var pose = NotchClawdPose()
        switch motion {
        case .rest: break
        case .content, .done: pose.eyes = .happy
        case .doze: pose.eyes = .closed; pose.bubble = .sleep(1)
        case .think, .slow: pose.arms = .typeLeft
        case .work: pose.legs = .stepLeft
        case .alert: pose.bubble = .alert; pose.arms = .waveUp
        case .ask: pose.bubble = .question; pose.arms = .waveUp
        }
        return pose
    }

    /// A crouch, a jump with both arms up, and a landing with smiling eyes.
    private static func cheerPose(at age: TimeInterval) -> NotchClawdPose {
        var pose = NotchClawdPose()
        pose.eyes = .happy
        let step = Int(age * Double(rate))
        let arc = [0, 0, 2, 4, 5, 5, 4, 2, 0]
        if step < arc.count {
            pose.jump = arc[step]
            pose.arms = step < 2 ? .down : .up
        } else {
            pose.arms = .up
        }
        return pose
    }

    /// Eyes shut for two steps every few seconds, at the Clawd's own pace.
    static func blinks(_ time: TimeInterval, seed: UInt64) -> Bool {
        let period = 3.4 + Double(seed % 23) / 10
        let offset = Double((seed >> 8) % 1000) / 1000 * period
        return (time + offset).truncatingRemainder(dividingBy: period) < 2 / Double(rate)
    }

    /// Now and then it looks to one side for a moment.
    private static func glance(_ time: TimeInterval, seed: UInt64) -> NotchClawdPose.Eyes {
        let period = 6.1 + Double((seed >> 16) % 31) / 10
        let offset = Double((seed >> 24) % 1000) / 1000 * period
        let shifted = time + offset
        guard shifted.truncatingRemainder(dividingBy: period) < 0.8 else { return .open }
        return Int(shifted / period) % 2 == 0 ? .left : .right
    }

    /// Up a pixel for the second half of each breath.
    private static func breath(_ time: TimeInterval, period: TimeInterval) -> Int {
        time.truncatingRemainder(dividingBy: period) < period / 2 ? 0 : 1
    }

    // MARK: Drawing

    /// A small drawing, its letters standing for colors, placed by its top
    /// left corner in the body's own pixels: the body's top left is 0, 0.
    struct Stamp {
        let x: Int
        let y: Int
        let rows: [String]
    }

    /// The costume in `pose`, as one letter per canvas pixel, nil where
    /// nothing is drawn. Rows run from the top.
    static func letters(_ costume: ClawdCostume, _ pose: NotchClawdPose,
                        character: ClawdCharacter = .clawd) -> [Character?] {
        compose(costume, pose, character: character).canvas
    }

    /// How many pixels of the costume in `pose` fall off the canvas: none,
    /// for a drawing that fits.
    static func clippedPixels(_ costume: ClawdCostume, _ pose: NotchClawdPose,
                              character: ClawdCharacter = .clawd) -> Int {
        compose(costume, pose, character: character).clipped
    }

    private static func compose(_ costume: ClawdCostume, _ pose: NotchClawdPose,
                                character: ClawdCharacter) -> (canvas: [Character?], clipped: Int) {
        var canvas = [Character?](repeating: nil, count: width * height)
        var clipped = 0
        let lift = pose.rise + pose.jump
        func put(_ stamp: Stamp, dy: Int = -lift) {
            for (row, line) in stamp.rows.enumerated() {
                for (column, letter) in line.enumerated() where letter != "." {
                    let x = bodyX + stamp.x + column, y = bodyY + stamp.y + row + dy
                    guard x >= 0, x < width, y >= 0, y < height else { clipped += 1; continue }
                    canvas[y * width + x] = letter
                }
            }
        }
        cape(costume.extra, tick: pose.tick).forEach { put($0) }
        // Legs reach the ground under a rising body, and tuck in off it.
        for (x, left) in feet(character) {
            let lifted = (pose.legs == .stepLeft && left) || (pose.legs == .stepRight && !left)
            let top = 8 - pose.rise, bottom = pose.jump > 0 || lifted ? 8 : 9
            put(Stamp(x: x, y: top, rows: Array(repeating: "c", count: max(1, bottom - top + 1))), dy: -pose.jump)
        }
        switch character {
        case .clawd:
            put(Stamp(x: 0, y: 0, rows: Array(repeating: String(repeating: "C", count: 13), count: 8)))
            arms(pose.arms).forEach { put($0) }
        case .codex:
            // The antenna goes first, so a hat simply covers it.
            codexAntenna.forEach { put($0) }
            put(Stamp(x: 0, y: 0, rows: codexBody))
            codexArms(pose.arms).forEach { put($0) }
        }
        put(eyes(pose.eyes, column: 3))
        put(eyes(pose.eyes, column: 9))
        if let stamp = eyewear(costume.eyewear) { put(stamp) }
        hat(costume.hat, tick: pose.tick).forEach { put($0) }
        extra(costume.extra, arms: pose.arms, tick: pose.tick, character: character).forEach { put($0) }
        if let bubble = pose.bubble { bubbleStamps(bubble).forEach { put($0) } }
        return (canvas, clipped)
    }

    private static func arms(_ arms: NotchClawdPose.Arms) -> [Stamp] {
        let downLeft = Stamp(x: -2, y: 4, rows: ["CC", "CC"])
        let downRight = Stamp(x: 13, y: 4, rows: ["CC", "CC"])
        let upLeft = Stamp(x: -3, y: 0, rows: ["CC.", "CC.", ".CC", ".CC", "..C"])
        let upRight = Stamp(x: 13, y: 0, rows: [".CC", ".CC", "CC.", "CC.", "C.."])
        switch arms {
        case .down: return [downLeft, downRight]
        case .up: return [upLeft, upRight]
        case .typeLeft: return [Stamp(x: -2, y: 5, rows: ["CC", "CC"]), downRight]
        case .typeRight: return [downLeft, Stamp(x: 13, y: 5, rows: ["CC", "CC"])]
        case .waveUp: return [downLeft, upRight]
        case .waveOut: return [downLeft, Stamp(x: 13, y: 2, rows: ["..C", ".CC", "CC.", "C.."])]
        }
    }

    /// Where each leg stands, and whether it is a left one.
    private static func feet(_ character: ClawdCharacter) -> [(x: Int, left: Bool)] {
        switch character {
        case .clawd: return [(0, true), (2, true), (10, false), (12, false)]
        // Two stubby feet, each two pixels wide, under the terminal.
        case .codex: return [(3, true), (4, true), (8, false), (9, false)]
        }
    }

    // MARK: Codex's robot

    /// Codex's figure in Clawd's 13 by 8 footprint: a terminal with rounded
    /// corners, its dark screen (V) where the eyes glow, a status light (G)
    /// and a shaded base.
    static let codexBody = [
        ".CCCCCCCCCCC.",
        "CCVVVVVVVVVCC",
        "CCVVVVVVVVVCC",
        "CCVVVVVVVVVCC",
        "CCVVVVVVVVVCC",
        "CCCCCCCCCCCCC",
        "CCCCCCCCCGcCC",
        ".ccccccccccc.",
    ]

    /// An antenna with a light on top.
    private static let codexAntenna = [Stamp(x: 6, y: -2, rows: ["G"]), Stamp(x: 6, y: -1, rows: ["c"])]

    /// Thin arms, a pixel wide, at the terminal's sides.
    private static func codexArms(_ arms: NotchClawdPose.Arms) -> [Stamp] {
        let downLeft = Stamp(x: -1, y: 3, rows: ["C", "C", "c"])
        let downRight = Stamp(x: 13, y: 3, rows: ["C", "C", "c"])
        let upLeft = Stamp(x: -1, y: -2, rows: ["c", "C", "C", "C"])
        let upRight = Stamp(x: 13, y: -2, rows: ["c", "C", "C", "C"])
        switch arms {
        case .down: return [downLeft, downRight]
        case .up: return [upLeft, upRight]
        case .typeLeft: return [Stamp(x: -2, y: 4, rows: [".C", "cC"]), downRight]
        case .typeRight: return [downLeft, Stamp(x: 13, y: 4, rows: ["C.", "Cc"])]
        case .waveUp: return [downLeft, upRight]
        case .waveOut: return [downLeft, Stamp(x: 13, y: 0, rows: [".c", "C.", "C."])]
        }
    }

    private static func eyes(_ eyes: NotchClawdPose.Eyes, column x: Int) -> Stamp {
        switch eyes {
        case .open: return Stamp(x: x, y: 2, rows: ["E", "E"])
        case .closed: return Stamp(x: x - 1, y: 3, rows: ["EEE"])
        case .happy: return Stamp(x: x - 1, y: 2, rows: [".E.", "E.E"])
        case .left: return Stamp(x: x - 1, y: 2, rows: ["E", "E"])
        case .right: return Stamp(x: x + 1, y: 2, rows: ["E", "E"])
        }
    }

    private static func eyewear(_ eyewear: ClawdEyewear) -> Stamp? {
        switch eyewear {
        case .none: return nil
        case .glasses:
            return Stamp(x: 0, y: 1, rows: [".KKKKK.KKKKK.",
                                            "KK...KKK...KK",
                                            ".K...K.K...K.",
                                            ".KKKKK.KKKKK."])
        case .shades:
            return Stamp(x: 0, y: 1, rows: ["KKKKKKKKKKKKK",
                                            ".LlLLL.LlLLL.",
                                            ".LLLLL.LLLLL.",
                                            "..LLL...LLL.."])
        case .round:
            return Stamp(x: 1, y: 1, rows: [".KKK...KKK.",
                                            "K...KKK...K",
                                            "K...K.K...K",
                                            ".KKK...KKK."])
        case .monocle:
            return Stamp(x: 7, y: 1, rows: [".YYY.",
                                            "Y...Y",
                                            "Y...Y",
                                            ".YYY.",
                                            "....y",
                                            "...y.",
                                            "...y."])
        case .anaglyph:
            return Stamp(x: 0, y: 1, rows: ["WWWWWWWWWWWWW",
                                            "WRRRRRWTTTTTW",
                                            "WRRRRRWTTTTTW",
                                            ".WWWWW.WWWWW."])
        case .batmanMask:
            return Stamp(x: 0, y: -3, rows: ["H...........H",
                                             "KH.........HK",
                                             "KKHHHHHHHHHKK",
                                             "KKKKKKKKKKKKK",
                                             "KKKKKKKKKKKKK",
                                             "KKWWWKKKWWWKK",
                                             "KKKKKKKKKKKKK",
                                             "KKKK.....KKKK",
                                             "KKK.......KKK",
                                             ".KK.......KK."])
        case .ironManMask:
            return Stamp(x: 0, y: 0, rows: [".RRRYYYYYRRR.",
                                            "RYYYYYYYYYYYR",
                                            "RYYTTYYYTTYYR",
                                            "RYYYYYYYYYYYR",
                                            "RRYYYYYYYYYRR",
                                            "RRYYYsssYYYRR",
                                            ".RRYYYYYYYRR.",
                                            "..RRRRRRRRR.."])
        case .spiderManMask:
            return Stamp(x: 0, y: -1, rows: [".RRRRRRRRRRR.",
                                            "RRRRRRKRRRRRR",
                                            "RKWWWKRKWWWKR",
                                            "RKKWWKRKWWKKR",
                                            "RRKWWKRKWWKRR",
                                            ".BBRRRKRRRBB."])
        case .doomMask:
            return Stamp(x: 0, y: 0, rows: [".sSSSSSSSSSs.",
                                            "sSSSSSSSSSSSs",
                                            "sKKKKSsSKKKKs",
                                            "sSSSSSsSSSSSs",
                                            "DSsSSSsSSSsSD",
                                            "sSKKKKKKKKKSs",
                                            ".sSKsKsKsKSs.",
                                            "..ssSSSSSss.."])
        case .wolverineMask:
            return Stamp(x: 0, y: -3, rows: ["H...........H",
                                             "KH.........HK",
                                             "KKH.......HKK",
                                             "KKKKYYYYYKKKK",
                                             "KKKKYYYYYKKKK",
                                             "KKWWWYYYWWWKK",
                                             "KKKYYYYYYYKKK",
                                             "YYYY.....YYYY",
                                             ".YYY.....YYY.",
                                             "..YY.....YY.."])
        case .flashMask:
            return Stamp(x: -2, y: -1, rows: ["....RRRRRRRRR....",
                                              "YY.RRRRRRRRRRR.YY",
                                              ".YYRRRRRRRRRRRYY.",
                                              "..RRWWWRRRWWWRR..",
                                              "..RRRRRRRRRRRRR..",
                                              "..RRRR.....RRRR..",
                                              "..RRR.......RRR.."])
        case .captainMask:
            return Stamp(x: 0, y: -2, rows: ["...BBBWBBB...",
                                             ".BBBBWBWBBBB.",
                                             "WBBBWWWWWBBBW",
                                             "WBBBWBBBWBBBW",
                                             "BWWWBBBBBWWWB",
                                             "BBBBBBBBBBBBB",
                                             "BBBB.....BBBB",
                                             ".BB.......BB."])
        case .deadpoolMask:
            return Stamp(x: 0, y: 0, rows: [".RRRRRRRRRRR.",
                                            "RRKKRRRRRKKRR",
                                            "RKKKKRRRKKKKR",
                                            "RKWWKRRRKWWKR",
                                            "RKKKKRRRKKKKR",
                                            "RRKKRRRRRKKRR",
                                            ".RRRRRKRRRRR.",
                                            "..RRRRRRRRR.."])
        case .venomMask:
            return Stamp(x: 0, y: 0, rows: [".HHHHHHHHHHH.",
                                            "KWKKKKKKKKKWK",
                                            "KWWKKKKKKKWWK",
                                            "KWWWKKKKKWWWK",
                                            "KKKKKKKKKKKKK",
                                            "KWKWKWKWKWKWK",
                                            ".KWKWKWKWKWK.",
                                            "..KKRRRRRKK.."])
        case .blackPantherMask:
            return Stamp(x: 0, y: -2, rows: ["H...........H",
                                             "KH.........HK",
                                             "KKHHHHHHHHHKK",
                                             "KsKKKKsKKKKsK",
                                             "KKsWWKsKWWsKK",
                                             "KKKKKsKsKKKKK",
                                             "KKsKKKsKKKsKK",
                                             "KKKsKKsKKsKKK",
                                             ".KKKsKsKsKKK.",
                                             "..KKKsssKKK.."])
        case .daredevilMask:
            return Stamp(x: 0, y: -2, rows: [".R.........R.",
                                             ".RR.......RR.",
                                             "RRRRRRRRRRRRR",
                                             "RRRRRRRRRRRRR",
                                             "RRKKKRRRKKKRR",
                                             "RRRKRRRRRKRRR",
                                             "RRRR.....RRRR",
                                             ".RRR.....RRR."])
        case .starLordMask:
            return Stamp(x: 0, y: 0, rows: [".sSSSSSSSSSs.",
                                            "sSSsssSsssSSs",
                                            "SsRRRRsRRRRsS",
                                            "SssRRRsRRRssS",
                                            "DSsssSSSsssSD",
                                            "DSSSsKKKsSSSD",
                                            ".sSSsKsKsSSs.",
                                            "..sSSSSSSSs.."])
        case .greenLanternMask:
            return Stamp(x: 0, y: 1, rows: ["Q.QQQQ.QQQQ.Q",
                                            "QQWWWQQQWWWQQ",
                                            ".QQWQQ.QQWQQ."])
        case .robinMask:
            return Stamp(x: 0, y: 1, rows: ["K.KKKK.KKKK.K",
                                            "KKWWWKKKWWWKK",
                                            ".KKWKK.KKWKK."])
        }
    }

    private static func hat(_ hat: ClawdHat, tick: Int) -> [Stamp] {
        switch hat {
        case .none: return []
        case .beanie:
            return [Stamp(x: 0, y: -4, rows: [".....WWW.....",
                                              "...AAAAAAA...",
                                              ".AAAAAAAAAAA.",
                                              "AAAAAAAAAAAAA",
                                              "aAaAaAaAaAaAa"])]
        case .cap:
            return [Stamp(x: 0, y: -3, rows: ["...AAAAAAA......",
                                              ".AAAAAWAAAAA....",
                                              "AAAAAAAAAAAAaaaa"])]
        case .beret:
            return [Stamp(x: -1, y: -4, rows: [".........a....",
                                               "...AAAAAAAAA..",
                                               ".AAAAAAAAAAAAA",
                                               "AAAAAAAAAAAAAA",
                                               ".aaaaaaaaaaa.."])]
        case .topHat:
            return [Stamp(x: 1, y: -6, rows: ["..DDDDDDD..",
                                              "..DdDDDDD..",
                                              "..DdDDDDD..",
                                              "..RRRRRRR..",
                                              "..DDDDDDD..",
                                              "DDDDDDDDDDD"])]
        case .crown:
            return [Stamp(x: 1, y: -4, rows: ["Y....Y....Y",
                                              "YY..YYY..YY",
                                              "YYYYYRYYYYY",
                                              "yyyyyyyyyyy"])]
        case .party:
            return [Stamp(x: 3, y: -7, rows: ["...Y...",
                                              "...A...",
                                              "..WWW..",
                                              "..AAA..",
                                              ".WWWWW.",
                                              ".AAAAA.",
                                              "WWWWWWW"])]
        case .propeller:
            let blades = tick == 0 ? "RRRR.s.BBBB" : "..RR.s.BB.."
            return [Stamp(x: 1, y: -5, rows: [blades,
                                              ".....s.....",
                                              "...RRYBB...",
                                              ".RRRRYBBBB.",
                                              "RRRRRYBBBBB"])]
        case .chef:
            return [Stamp(x: 1, y: -6, rows: ["..WWW.WWW..",
                                              ".WWWWWWWWW.",
                                              "WWWWWWWWWWW",
                                              ".WWWWWWWWW.",
                                              "..WWWWWWW..",
                                              "..wwwwwww.."])]
        case .doomHood:
            return [Stamp(x: -1, y: -3, rows: [".....QQQQQ.....",
                                               "...QQQQQQQQQ...",
                                               ".QQQQqqqqqQQQQ.",
                                               ".QQqq.....qqQQ.",
                                               "QQq.........qQQ",
                                               "QQ...........QQ",
                                               "QQ...........QQ",
                                               "Qq...........qQ",
                                               "q.............q"])]
        case .thorHelmet:
            return [Stamp(x: -2, y: -5, rows: ["W...............W",
                                            "WW.............WW",
                                            ".WW..SSSSSSS..WW.",
                                            "..WWSSSSSSSSSWW..",
                                            "...SSSSSSSSSSS..."])]
        case .lokiHorns:
            return [Stamp(x: 0, y: -7, rows: ["Y...........Y",
                                            "YY.........YY",
                                            ".Y.........Y.",
                                            ".YY.......YY.",
                                            "..YY.....YY..",
                                            "...YYYYYYY...",
                                            "...QQQYQQQ..."])]
        case .wonderTiara:
            return [Stamp(x: 0, y: -2, rows: ["Y.....Y.....Y",
                                            "YYYYYYYYYYYYY",
                                            ".YYYYRYYYYYY."])]
        }
    }

    /// Where the right hand is in each pose, for what it holds.
    private static func rightHand(_ arms: NotchClawdPose.Arms, character: ClawdCharacter) -> (x: Int, y: Int) {
        if character == .codex {
            // The robot's arms are thinner and sit a pixel closer.
            switch arms {
            case .down, .typeLeft: return (14, 3)
            case .typeRight: return (14, 4)
            case .up, .waveUp: return (14, -3)
            case .waveOut: return (15, -1)
            }
        }
        switch arms {
        case .down, .typeLeft: return (15, 3)
        case .typeRight: return (15, 4)
        case .up, .waveUp: return (15, -2)
        case .waveOut: return (16, 0)
        }
    }

    // Capes hang from the shoulders to the ground behind the legs and arms,
    // in shadow, edged in a bright color so they stand out on the island's
    // black and on a shell of their own color.
    // Clasps are drawn in front, with the other extras.
    private static let capeRows = ["...eFFFFFFFFFFFFFe...",
                                   "...eFFFFFFFFFFFFFe...",
                                   "..eFFFFFFFFFFFFFFFe..",
                                   "..eFFFFFFFFFFFFFFFe..",
                                   ".eFFFFFFFFFFFFFFFFFe.",
                                   ".eFFFFFFFFFFFFFFFFFe.",
                                   "eFFFFFFFFFFFFFFFFFFFe",
                                   "eFFFFFFFFFFFFFFFFFFFe",
                                   "eFFFFFFFFFFFFFFFFFFFe"]
    private static let capeHems = ["eFF.FFFF.FFF.FFFF.FFe",
                                   "eF.FFFF.FFFFF.FFFF.Fe"]
    private static let batCapeHem = "eF.FF.FF.FFF.FF.FF.Fe"

    private static func cape(_ extra: ClawdExtra, tick: Int) -> [Stamp] {
        let fill: Character, edge: Character
        switch extra {
        case .doomCape: (fill, edge) = ("q", "Q")
        case .strangeCape: (fill, edge) = ("r", "Y")
        case .thorCape, .supermanCape: (fill, edge) = ("r", "R")
        case .batmanCape: (fill, edge) = ("K", "H")
        default: return []
        }
        let hem = extra == .batmanCape ? batCapeHem : capeHems[tick == 0 ? 0 : 1]
        let drawn = (capeRows + [hem]).map { row in
            String(row.map { $0 == "F" ? fill : $0 == "e" ? edge : $0 })
        }
        var stamps = [Stamp(x: -4, y: 0, rows: drawn)]
        if extra == .strangeCape {
            // The Cloak of Levitation's high collar stands up beside the head.
            stamps += [Stamp(x: -2, y: -3, rows: ["y..", "Ry.", "RRy"]),
                       Stamp(x: 12, y: -3, rows: ["..y", ".yR", "yRR"])]
        }
        return stamps
    }

    private static func extra(_ extra: ClawdExtra, arms: NotchClawdPose.Arms, tick: Int,
                              character: ClawdCharacter) -> [Stamp] {
        switch extra {
        case .none: return []
        case .bow:
            return [Stamp(x: 9, y: -2, rows: ["AA.AA", "AAaAA", "AA.AA"])]
        case .scarf:
            return [Stamp(x: 0, y: 4, rows: ["AAAAAAAAAAAAA",
                                             "aaaaaaaaaaaaa",
                                             ".........AA..",
                                             ".........aA.."])]
        case .bowTie:
            return [Stamp(x: 4, y: 4, rows: ["KK.KK", "KKsKK", "KK.KK"])]
        case .flower:
            return [Stamp(x: -1, y: -2, rows: [".W.", "WYW", ".W."])]
        case .headphones:
            return [Stamp(x: -2, y: -3, rows: ["....SSSSSSSSS....",
                                               "..SS.........SS..",
                                               ".S.............S.",
                                               ".S.............S.",
                                               "AA.............AA",
                                               "AA.............AA",
                                               "AA.............AA"])]
        case .coffee:
            let hand = rightHand(arms, character: character)
            let steam = tick == 0 ? [".w.", "w.."] : ["w..", ".w."]
            return [Stamp(x: hand.x, y: hand.y - 2, rows: steam + ["NNN", "WWW", "WWW"])]
        case .doomCape:
            return [Stamp(x: 0, y: 0, rows: ["QQY.......YQQ"])]
        case .strangeCape:
            return [Stamp(x: 0, y: 0, rows: ["RRR.......RRR",
                                            "RRRY.....YRRR"])]
        case .thorCape:
            return [Stamp(x: 0, y: 0, rows: ["SS.........SS",
                                             "SS.........SS"])]
        case .supermanCape:
            return [Stamp(x: 0, y: 0, rows: ["RR.........RR"])]
        case .batmanCape:
            return [Stamp(x: 0, y: 0, rows: ["HK.........KH"])]
        case .captainShield:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 10, y: hand.y - 4, rows: ["..RRRRR..",
                                                       ".RWWWWWR.",
                                                       "RWRRRRRWR",
                                                       "RWRBWBRWR",
                                                       "RWRWWWRWR",
                                                       "RWRBWBRWR",
                                                       "RWRRRRRWR",
                                                       ".RWWWWWR.",
                                                       "..RRRRR.."])]
        case .mjolnir:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 10, y: hand.y - 4, rows: ["SSSSSSSSS",
                                            "SwwwwwwwS",
                                            "SSSSSSSSS",
                                            "....N....",
                                            "....N....",
                                            "....N....",
                                            "....Y...."])]
        case .aquamanTrident:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 12, y: hand.y - 4, rows: ["Y..Y..Y",
                                            "Y..Y..Y",
                                            ".YYYYY.",
                                            "...Y...",
                                            "...Y...",
                                            "...Y...",
                                            "...Y...",
                                            "...Y...",
                                            "...Y..."])]
        case .lanternRing:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 12, y: hand.y - 4, rows: ["..QQQ..",
                                            ".QWWWQ.",
                                            "QWWWWWQ",
                                            "QWQQQWQ",
                                            "QWQWQWQ",
                                            ".QWWWQ.",
                                            "..QQQ.."])]
        case .wolverineClaws:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 10, y: hand.y - 4, rows: ["S.S.S",
                                            "S.S.S",
                                            "S.S.S",
                                            "S.S.S",
                                            ".SSS.",
                                            ".NNN."])]
        case .batarang:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 10, y: hand.y - 4, rows: ["d..d.d..d",
                                                       "Dd.DDD.dD",
                                                       "DDDDDDDDD",
                                                       ".DDDDDDD.",
                                                       "..D.D.D.."])]
        case .hawkeyeBow:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 12, y: hand.y - 4, rows: ["wP.....",
                                                       "w.P....",
                                                       "w..P...",
                                                       "w..P.W.",
                                                       "SSSSSSW",
                                                       "w..P.W.",
                                                       "w..P...",
                                                       "w.P....",
                                                       "wP....."])]
        case .strangeSpell:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 10, y: hand.y - 4, rows: ["..YYY..",
                                            ".Y...Y.",
                                            "Y.YYY.Y",
                                            "Y.Y.Y.Y",
                                            "Y.YYY.Y",
                                            ".Y...Y.",
                                            "..YYY.."])]
        case .webShooter:
            let hand = rightHand(arms, character: character)
            return [Stamp(x: 10, y: hand.y - 4, rows: ["W..W..W",
                                            ".W.W.W.",
                                            "..WWW..",
                                            "WWWWWWW",
                                            "..WWW..",
                                            ".W.W.W.",
                                            "W..W..W"])]
        case .ironReactor:
            return [Stamp(x: 4, y: 4, rows: [".TTT.",
                                             "TWWWT",
                                             "TWWWT",
                                             ".TTT."])]
        case .supermanEmblem:
            return [Stamp(x: 4, y: 3, rows: ["YYYYY",
                                             "YRRRY",
                                             "YRYYY",
                                             "YYYRY",
                                             "YRRRY"])]
        case .flashEmblem:
            return [Stamp(x: 4, y: 3, rows: [".WWW.",
                                             "WWWYW",
                                             "WYYYW",
                                             "WYWWW",
                                             ".WWW."])]
        case .lanternEmblem:
            return [Stamp(x: 4, y: 3, rows: [".WWW.",
                                             "WQQQW",
                                             "WQWQW",
                                             "WQQQW",
                                             ".WWW."])]
        }
    }

    private static func bubbleStamps(_ bubble: NotchClawdPose.Bubble) -> [Stamp] {
        switch bubble {
        case .alert:
            return [Stamp(x: 15, y: -7, rows: ["XX", "XX", "XX", "..", "XX"])]
        case .question:
            return [Stamp(x: 15, y: -7, rows: ["XXX", "..X", ".X.", "...", ".X."])]
        case .sleep(let phase):
            let small = ["ZZZZ", "..Z.", ".Z..", "ZZZZ"], large = ["ZZZZZ", "...Z.", "..Z..", ".Z...", "ZZZZZ"]
            switch phase % 3 {
            case 0: return [Stamp(x: 13, y: -3, rows: small)]
            case 1: return [Stamp(x: 13, y: -3, rows: small), Stamp(x: 14, y: -9, rows: large)]
            default: return [Stamp(x: 14, y: -9, rows: large)]
            }
        }
    }

    // MARK: Colors

    /// Codex's screen, and the light its eyes and status lamp give off.
    static let codexScreen = (red: 16, green: 22, blue: 28)
    static let codexGlow = (red: 124, green: 240, blue: 204)

    /// sRGB out of 255 for each letter; the shell and accent letters come
    /// from the costume.
    static func color(_ letter: Character, costume: ClawdCostume,
                      character: ClawdCharacter = .clawd) -> (red: Int, green: Int, blue: Int)? {
        if character == .codex {
            // The eyes glow on the dark screen instead of sitting dark on the shell.
            switch letter {
            case "V": return codexScreen
            case "E", "G": return codexGlow
            default: break
            }
        }
        let shell = costume.shell.rgb, accent = accent(for: costume.shell).rgb
        switch letter {
        case "C": return shell
        case "c": return shaded(shell)
        case "A": return accent
        case "a": return shaded(accent)
        case "Q": return (44, 142, 76)
        // A blue sheen that keeps black parts from vanishing on the island.
        case "H": return (64, 80, 124)
        case "r": return (150, 36, 44)
        case "P": return (150, 84, 200)
        case "q": return (26, 84, 48)
        case "E": return (24, 22, 26)
        case "K": return (32, 32, 38)
        case "D": return (66, 66, 78)
        case "d": return (108, 108, 124)
        case "L": return (30, 30, 40)
        case "l": return (150, 170, 205)
        case "W": return (246, 244, 238)
        case "w": return (196, 200, 210)
        case "R": return (226, 64, 64)
        case "Y": return (247, 198, 70)
        case "y": return (196, 142, 44)
        case "T": return (64, 204, 232)
        case "N": return (124, 76, 44)
        case "B": return (72, 132, 232)
        case "S": return (156, 158, 170)
        case "s": return (98, 100, 112)
        case "X": return (255, 199, 71)
        case "Z": return (196, 206, 236)
        default: return nil
        }
    }

    /// The color hats, scarves and headphones come in on each shell: one
    /// far enough from it to stand out.
    static func accent(for shell: ClawdShell) -> ClawdShell {
        switch shell {
        case .midnight: return .sky
        case .clay: return .sky
        case .coral: return .teal
        case .amber: return .iris
        case .moss: return .coral
        case .mint: return .orchid
        case .teal: return .amber
        case .sky: return .coral
        case .iris: return .amber
        case .orchid: return .mint
        case .rose: return .sky
        }
    }

    private static func shaded(_ color: (red: Int, green: Int, blue: Int)) -> (red: Int, green: Int, blue: Int) {
        (color.red * 78 / 100, color.green * 78 / 100, color.blue * 78 / 100)
    }

    /// Straight RGBA bytes for the canvas, transparent where nothing is drawn.
    static func pixels(_ costume: ClawdCostume, _ pose: NotchClawdPose, character: ClawdCharacter = .clawd) -> [UInt8] {
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        for (index, letter) in letters(costume, pose, character: character).enumerated() {
            guard let letter, let color = color(letter, costume: costume, character: character) else { continue }
            bytes[index * 4] = UInt8(clamping: color.red)
            bytes[index * 4 + 1] = UInt8(clamping: color.green)
            bytes[index * 4 + 2] = UInt8(clamping: color.blue)
            bytes[index * 4 + 3] = 255
        }
        return bytes
    }

    // MARK: Images

    private static let cache: NSCache<NSString, CGImage> = {
        let cache = NSCache<NSString, CGImage>()
        cache.countLimit = 512
        return cache
    }()

    /// The canvas as an image, a pixel per canvas pixel, made once per
    /// character, costume and pose. Every pixel is either opaque or clear.
    static func image(_ costume: ClawdCostume, _ pose: NotchClawdPose, character: ClawdCharacter = .clawd) -> CGImage? {
        let key = "\(character.rawValue)|\(costume.code)|\(pose.key)" as NSString
        if let image = cache.object(forKey: key) { return image }
        let data = Data(pixels(costume, pose, character: character))
        guard let provider = CGDataProvider(data: data as CFData),
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                                  bytesPerRow: width * 4, space: space,
                                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
        else { return nil }
        cache.setObject(image, forKey: key)
        return image
    }
}
