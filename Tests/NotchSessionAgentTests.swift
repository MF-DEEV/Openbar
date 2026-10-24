// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import CoreGraphics
import Foundation

/// Claude Code and Codex sessions share the Sessions list; each reads as
/// its own agent: its figure, its mark and its color.
enum NotchSessionAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("sessions by agent") { byAgent(suite) }
        suite.run("svg path data") { pathData(suite) }
        suite.run("codex mark") { codexMark(suite) }
    }

    static func session(_ id: String, _ phase: ClaudeSessionPhase, provider: ClaudeSessionProvider) -> ClaudeSession {
        var session = ClaudeSession(id: id, pid: 7, cwd: "/x/app", project: "app", name: "app",
                                    host: .terminal(tty: "/dev/ttys001"), phase: phase,
                                    phaseSince: Date(timeIntervalSinceReferenceDate: 0), turnStarted: nil,
                                    timeline: [], subagents: [], tokens: AgentTokens(), cost: 0, hooked: false)
        session.provider = provider
        return session
    }

    static func byAgent(_ suite: TestSuite) {
        let mixed = ClaudeSessionCounts(working: 3, workingCodex: 1, waiting: 1, finished: 2)
        let reading = NotchClaudeSessionsSupport.stripReading(mixed)
        suite.expect(reading == [.init(phase: .waitingPermission, count: 1),
                                 .init(phase: .generating, count: 2, provider: .claude),
                                 .init(phase: .generating, count: 1, provider: .codex),
                                 .init(phase: .finished, count: 2)],
                     "the counts split the sessions at work by agent, between waiting and finished: \(reading)")
        suite.expect(Set(reading.map(\.id)).count == reading.count, "each count is told apart from the others")
        suite.expect(NotchClaudeSessionsSupport.stripReading(ClaudeSessionCounts(working: 1, workingCodex: 1))
                     == [.init(phase: .generating, count: 1, provider: .codex)]
                     && NotchClaudeSessionsSupport.stripReading(ClaudeSessionCounts(working: 2))
                     == [.init(phase: .generating, count: 2, provider: .claude)],
                     "an agent with nothing at work is left out")
        suite.expect(NotchClaudeSessionsSupport.stripReading(ClaudeSessionCounts(working: 1, workingCodex: 5))
                     == [.init(phase: .generating, count: 1, provider: .codex)]
                     && NotchClaudeSessionsSupport.stripReading(ClaudeSessionCounts(working: -2, workingCodex: -1)).isEmpty,
                     "counts that make no sense never count a session twice or below none")
        suite.expect(NotchClaudeSessionsSupport.symbol(reading[1].phase) == "sparkle" && reading[1].phase == reading[2].phase,
                     "both agents' counts keep the phase's sparkle, so the mark shows only once")

        suite.expect(NotchClaudeSessionsSupport.stripAgent(ClaudeSessionCounts(waiting: 2, finished: 1)) == nil
                     && NotchClaudeSessionsSupport.stripAgent(ClaudeSessionCounts(working: 2, workingCodex: 1)) == .claude
                     && NotchClaudeSessionsSupport.stripAgent(ClaudeSessionCounts(working: 2)) == .claude
                     && NotchClaudeSessionsSupport.stripAgent(ClaudeSessionCounts(working: 2, workingCodex: 2)) == .codex,
                     "the strip shows Claude's mark while any Claude session works, Codex's when only Codex does")

        let joint = ClaudeSessionCounts(working: 3, finished: 1)
        let split = ClaudeSessionCounts(working: 3, workingCodex: 1, finished: 1)
        suite.expect(NotchClaudeSessionsSupport.stripText(split) == "• 2  • 1  • 1"
                     && NotchClaudeSessionsSupport.stripText(joint) == "• 3  • 1",
                     "the text the strip is measured by has a part for each agent at work")
        suite.expect(NotchClaudeSessionsSupport.stripReadingWidth(split, textSize: 13)
                     > NotchClaudeSessionsSupport.stripReadingWidth(joint, textSize: 13),
                     "and the strip makes room for the second count")
        let counted = ClaudeSessionCounts([session("a", .generating, provider: .claude),
                                           session("b", .runningTool, provider: .codex),
                                           session("c", .background, provider: .codex),
                                           session("d", .waitingPermission, provider: .codex)])
        suite.expect(counted == ClaudeSessionCounts(working: 3, workingCodex: 2, waiting: 1),
                     "a Codex session at work counts for Codex, and waiting counts for everyone")

        let codex = session("019a-codex", .idle, provider: .codex)
        let claude = session("019a-codex", .idle, provider: .claude)
        suite.expect(NotchClaudeSessionsSupport.costume(for: codex, kept: [:]) == ClawdCostume.seeded(codex.id, character: .codex)
                     && NotchClaudeSessionsSupport.costume(for: claude, kept: [:]) == ClawdCostume.seeded(claude.id),
                     "a session wears its own figure's lot")
        let chosen = ClawdCostume(shell: .moss, hat: .crown, eyewear: .none, extra: .scarf)
        suite.expect(NotchClaudeSessionsSupport.costume(for: codex, kept: [codex.id: chosen.code]) == chosen,
                     "or what the person dressed it in")

        let text = NotchClaudeSessionStrings.enUS
        suite.expect(text.replyPlaceholder(for: .codex) == "Reply to Codex…" && text.replyPlaceholder(for: .claude) == text.replyPlaceholder
                     && text.question(for: .codex) == "Codex asks" && text.question(for: .claude) == "Claude asks"
                     && text.previewsOff(for: .codex).contains("Codex") && text.previewsOff(for: .claude) == text.previewsOff,
                     "a Codex session is spoken of as Codex, a Claude Code one as before")
        suite.expect(AppLanguage.allCases.allSatisfy {
            FeatureStrings.notchClaudeSessions($0).replyPlaceholder(for: .codex) == text.replyPlaceholderCodex
        }, "every language has the Codex wording")
    }

    static func pathData(_ suite: TestSuite) {
        func box(_ data: String) -> CGRect? { NotchSVGPath.path(data)?.boundingBoxOfPath }
        func near(_ rect: CGRect?, _ expected: CGRect, _ tolerance: CGFloat = 0.01) -> Bool {
            guard let rect else { return false }
            return abs(rect.minX - expected.minX) <= tolerance && abs(rect.minY - expected.minY) <= tolerance
                && abs(rect.maxX - expected.maxX) <= tolerance && abs(rect.maxY - expected.maxY) <= tolerance
        }
        suite.expect(near(box("M0 0L10 0L10 10Z"), CGRect(x: 0, y: 0, width: 10, height: 10)), "lines draw where they say")
        suite.expect(near(box("m1 1h4v4h-4z"), CGRect(x: 1, y: 1, width: 4, height: 4))
                     && near(box("M1 1H5V5H1z"), CGRect(x: 1, y: 1, width: 4, height: 4)),
                     "relative and absolute horizontal and vertical lines agree")
        suite.expect(near(box("M0-.5.5.5"), CGRect(x: 0, y: -0.5, width: 0.5, height: 1))
                     && near(box("M 1e1,0 L 2E-1 0 l0 3"), CGRect(x: 0.2, y: 0, width: 9.8, height: 3)),
                     "numbers run together, with signs, points and exponents, read as minified SVG writes them")
        suite.expect(near(box("m2 2 3 0 0 3"), CGRect(x: 2, y: 2, width: 3, height: 3)),
                     "pairs after a move are lines")
        suite.expect(near(box("M0 0C0 10 10 10 10 0S20-10 20 0"), CGRect(x: 0, y: -7.5, width: 20, height: 15))
                     && near(box("M0 0Q5 10 10 0T20 0"), CGRect(x: 0, y: -5, width: 20, height: 10)),
                     "smooth curves mirror the control point before them")

        // A circle in two arcs, flags written apart and run together.
        let circle = CGRect(x: 0, y: 0, width: 20, height: 20)
        suite.expect(near(box("M0 10A10 10 0 1 1 20 10A10 10 0 1 1 0 10Z"), circle, 0.05)
                     && near(box("M0 10a10 10 0 1120 0a10 10 0 11-20 0z"), circle, 0.05),
                     "arcs read their flags with or without a space after them")
        if let round = NotchSVGPath.path("M0 10a10 10 0 1120 0a10 10 0 11-20 0z") {
            let inside = [CGPoint(x: 10, y: 10), CGPoint(x: 16.9, y: 16.9), CGPoint(x: 0.2, y: 10)]
            let outside = [CGPoint(x: 17.5, y: 17.5), CGPoint(x: 1, y: 1)]
            suite.expect(inside.allSatisfy { round.contains($0) } && !outside.contains { round.contains($0) },
                         "an arc follows its ellipse within a fraction of a point")
        } else {
            suite.expect(false, "a circle reads")
        }
        // A quarter disc: the sweep flag picks which way round, so which
        // centre, the arc takes.
        if let pie = NotchSVGPath.path("M10 10L0 10A10 10 0 0110 0Z"),
           let other = NotchSVGPath.path("M10 10L0 10A10 10 0 0010 0Z") {
            suite.expect(pie.contains(CGPoint(x: 3.5, y: 3.5)) && !pie.contains(CGPoint(x: 2.5, y: 2.5)),
                         "a sweep of 1 turns the way y points, around the far centre")
            suite.expect(other.contains(CGPoint(x: 8, y: 8)) && !other.contains(CGPoint(x: 3.5, y: 3.5)),
                         "and a sweep of 0 the other way, around the near one")
        } else {
            suite.expect(false, "a quarter disc reads")
        }
        suite.expect(near(box("M0 0A0 5 0 0 1 10 0"), CGRect(x: 0, y: 0, width: 10, height: 0))
                     && near(box("M0 0A1 1 0 0 1 10 0"), CGRect(x: 0, y: -5, width: 10, height: 5), 0.05),
                     "an arc with no radius is a line, and one too small to reach grows until it does")
        for broken in ["", "   ", "10 10", "M0", "M0 0 L", "X1 2", "M0 0 Z 1 1", "M0 0 a1 1 0 2 1 3 3", "M0 0Lx"] {
            suite.expect(NotchSVGPath.path(broken) == nil, "\"\(broken)\" does not read as a path")
        }
    }

    static func codexMark(_ suite: TestSuite) {
        let path = NotchCodexMark.path
        let bounds = path.boundingBoxOfPath
        suite.expect(!path.isEmpty && bounds.minX >= -0.01 && bounds.minY >= -0.01 && bounds.maxX <= 24.01 && bounds.maxY <= 24.01
                     && bounds.width > 23 && bounds.height > 23,
                     "Codex's mark fills its 24-point square: \(bounds)")
        var moves = 0
        path.applyWithBlock { if $0.pointee.type == .moveToPoint { moves += 1 } }
        suite.expect(moves == 3, "it is the cloud, the prompt's chevron and its underscore: \(moves) parts")
        let cloud = [CGPoint(x: 12, y: 3), CGPoint(x: 4, y: 12), CGPoint(x: 20, y: 12), CGPoint(x: 12, y: 21)]
        let holes = [CGPoint(x: 15.1, y: 15.4), CGPoint(x: 8, y: 11.5)]
        suite.expect(cloud.allSatisfy { path.contains($0, using: .evenOdd) }
                     && !holes.contains { path.contains($0, using: .evenOdd) }
                     && !path.contains(CGPoint(x: 0.4, y: 0.4), using: .evenOdd),
                     "the cloud is filled and the prompt is cut out of it")

        // Drawn as the strip and the badge draw it, on device pixels.
        for (points, scale) in [(9, 2), (14, 2), (18, 2), (18, 1)] {
            let side = points * scale
            guard let space = CGColorSpace(name: CGColorSpace.sRGB),
                  let context = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: side * 4,
                                          space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue),
                  let data = context.data
            else { suite.expect(false, "a bitmap for the mark"); continue }
            // Y pointing down, as in the flipped image the island draws.
            context.translateBy(x: 0, y: CGFloat(side))
            context.scaleBy(x: 1, y: -1)
            NotchCodexMark.draw(in: context, rect: CGRect(x: 0, y: 0, width: side, height: side))
            let bytes = data.bindMemory(to: UInt8.self, capacity: side * side * 4)
            func pixel(_ x: CGFloat, _ y: CGFloat) -> (red: Int, green: Int, blue: Int, alpha: Int) {
                let column = min(side - 1, Int(x / 24 * CGFloat(side))), row = min(side - 1, Int(y / 24 * CGFloat(side)))
                let index = (row * side + column) * 4
                return (Int(bytes[index]), Int(bytes[index + 1]), Int(bytes[index + 2]), Int(bytes[index + 3]))
            }
            let alphas = (0..<side * side).map { Int(bytes[$0 * 4 + 3]) }
            let covered = Double(alphas.reduce(0, +)) / Double(255 * side * side)
            suite.expect(covered > 0.55 && covered < 0.85 && alphas.contains(255) && alphas.contains(0),
                         "\(points) pt at \(scale)x: the mark covers its square as a cloud does: \(covered)")
            let top = pixel(12, 2), bottom = pixel(12, 21.5)
            suite.expect(top.alpha == 255 && bottom.alpha == 255 && top.red > bottom.red + 60 && top.blue >= 240,
                         "\(points) pt at \(scale)x: violet at the top turns to blue at the foot: \(top) \(bottom)")
            if side >= 28 {
                suite.expect(pixel(15.1, 15.4).alpha < 64 && pixel(4, 12).alpha == 255,
                             "\(points) pt at \(scale)x: the prompt reads as a cut, sharp against the cloud")
            }
        }
    }
}
