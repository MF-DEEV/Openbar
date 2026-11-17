// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import CoreGraphics
import Foundation

/// SVG path data read into a `CGPath`, for the marks that ship with the app
/// as path data: every command (M L H V C S Q T A Z), absolute and relative,
/// with numbers run together the way minified SVG writes them ("-.5.5" is
/// two numbers, and an arc's flags may touch, as in "a1 1 0 01.5.5").
/// Coordinates stay as written, y pointing down.
enum NotchSVGPath {
    /// The path `data` draws, or nil for data that does not read as a path.
    static func path(_ data: String) -> CGPath? {
        var reader = Reader(bytes: Array(data.utf8))
        let path = CGMutablePath()
        var current = CGPoint.zero
        var start = CGPoint.zero
        // The last control point, which S and T reflect.
        var lastCubic: CGPoint?
        var lastQuad: CGPoint?
        var command: UInt8?
        while true {
            reader.skipSeparators()
            guard !reader.atEnd else { break }
            if let letter = reader.command() {
                command = letter
            } else if command == nil || command == UInt8(ascii: "z") || command == UInt8(ascii: "Z") {
                // Numbers with no command before them, or after a close.
                return nil
            }
            guard let letter = command else { return nil }
            let relative = letter >= UInt8(ascii: "a")
            let origin = relative ? current : .zero
            func point() -> CGPoint? {
                guard let x = reader.number(), let y = reader.number() else { return nil }
                return CGPoint(x: origin.x + x, y: origin.y + y)
            }
            var cubic: CGPoint?
            var quad: CGPoint?
            switch letter | 0x20 {
            case UInt8(ascii: "m"):
                guard let to = point() else { return nil }
                path.move(to: to)
                current = to
                start = to
                // Pairs after the first are lines.
                command = relative ? UInt8(ascii: "l") : UInt8(ascii: "L")
            case UInt8(ascii: "l"):
                guard let to = point() else { return nil }
                path.addLine(to: to)
                current = to
            case UInt8(ascii: "h"):
                guard let x = reader.number() else { return nil }
                current = CGPoint(x: origin.x + x, y: current.y)
                path.addLine(to: current)
            case UInt8(ascii: "v"):
                guard let y = reader.number() else { return nil }
                current = CGPoint(x: current.x, y: origin.y + y)
                path.addLine(to: current)
            case UInt8(ascii: "c"):
                guard let first = point(), let second = point(), let to = point() else { return nil }
                path.addCurve(to: to, control1: first, control2: second)
                cubic = second
                current = to
            case UInt8(ascii: "s"):
                let first = lastCubic.map { reflect($0, around: current) } ?? current
                guard let second = point(), let to = point() else { return nil }
                path.addCurve(to: to, control1: first, control2: second)
                cubic = second
                current = to
            case UInt8(ascii: "q"):
                guard let control = point(), let to = point() else { return nil }
                path.addQuadCurve(to: to, control: control)
                quad = control
                current = to
            case UInt8(ascii: "t"):
                let control = lastQuad.map { reflect($0, around: current) } ?? current
                guard let to = point() else { return nil }
                path.addQuadCurve(to: to, control: control)
                quad = control
                current = to
            case UInt8(ascii: "a"):
                guard let rx = reader.number(), let ry = reader.number(), let rotation = reader.number(),
                      let large = reader.flag(), let sweep = reader.flag(), let to = point() else { return nil }
                addArc(to: path, from: current, to: to, radii: CGSize(width: rx, height: ry), rotation: rotation,
                       large: large, sweep: sweep)
                current = to
            case UInt8(ascii: "z"):
                path.closeSubpath()
                current = start
            default:
                return nil
            }
            lastCubic = cubic
            lastQuad = quad
        }
        return path.isEmpty ? nil : path.copy()
    }

    private static func reflect(_ point: CGPoint, around center: CGPoint) -> CGPoint {
        CGPoint(x: 2 * center.x - point.x, y: 2 * center.y - point.y)
    }

    /// An elliptical arc as cubic curves, a quarter turn at most each, after
    /// the SVG specification's conversion from end points to a centre.
    private static func addArc(to path: CGMutablePath, from: CGPoint, to: CGPoint, radii: CGSize, rotation: CGFloat,
                               large: Bool, sweep: Bool) {
        guard from != to else { return }
        var rx = abs(radii.width), ry = abs(radii.height)
        guard rx > 0, ry > 0 else { path.addLine(to: to); return }
        let phi = rotation * .pi / 180
        let cosPhi = cos(phi), sinPhi = sin(phi)
        let dx = (from.x - to.x) / 2, dy = (from.y - to.y) / 2
        let x1 = cosPhi * dx + sinPhi * dy
        let y1 = -sinPhi * dx + cosPhi * dy
        // Radii too small to reach are scaled up just enough.
        let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry)
        if lambda > 1 {
            let grow = lambda.squareRoot()
            rx *= grow
            ry *= grow
        }
        let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
        let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1
        var factor = denominator > 0 ? (max(0, numerator) / denominator).squareRoot() : 0
        if large == sweep { factor = -factor }
        let cx1 = factor * rx * y1 / ry
        let cy1 = -factor * ry * x1 / rx
        let center = CGPoint(x: cosPhi * cx1 - sinPhi * cy1 + (from.x + to.x) / 2,
                             y: sinPhi * cx1 + cosPhi * cy1 + (from.y + to.y) / 2)
        func angle(_ ux: CGFloat, _ uy: CGFloat, _ vx: CGFloat, _ vy: CGFloat) -> CGFloat {
            atan2(ux * vy - uy * vx, ux * vx + uy * vy)
        }
        let start = angle(1, 0, (x1 - cx1) / rx, (y1 - cy1) / ry)
        var delta = angle((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry)
        if !sweep, delta > 0 { delta -= 2 * .pi }
        if sweep, delta < 0 { delta += 2 * .pi }
        let segments = max(1, Int((abs(delta) / (.pi / 2)).rounded(.up)))
        let step = delta / CGFloat(segments)
        // How far along the tangent a quarter turn's control points sit.
        let handle = 4 / 3 * tan(step / 4)
        func onEllipse(_ theta: CGFloat) -> CGPoint {
            let x = rx * cos(theta), y = ry * sin(theta)
            return CGPoint(x: center.x + cosPhi * x - sinPhi * y, y: center.y + sinPhi * x + cosPhi * y)
        }
        func tangent(_ theta: CGFloat) -> CGPoint {
            let x = -rx * sin(theta), y = ry * cos(theta)
            return CGPoint(x: cosPhi * x - sinPhi * y, y: sinPhi * x + cosPhi * y)
        }
        var theta = start
        for index in 0..<segments {
            let next = theta + step
            let a = onEllipse(theta), b = index == segments - 1 ? to : onEllipse(next)
            let ta = tangent(theta), tb = tangent(next)
            path.addCurve(to: b, control1: CGPoint(x: a.x + handle * ta.x, y: a.y + handle * ta.y),
                          control2: CGPoint(x: b.x - handle * tb.x, y: b.y - handle * tb.y))
            theta = next
        }
    }

    private struct Reader {
        let bytes: [UInt8]
        var index = 0

        var atEnd: Bool { index >= bytes.count }

        mutating func skipSeparators() {
            while index < bytes.count, [0x20, 0x09, 0x0a, 0x0d, 0x0c, UInt8(ascii: ",")].contains(bytes[index]) {
                index += 1
            }
        }

        /// A command letter, if one comes next.
        mutating func command() -> UInt8? {
            guard index < bytes.count else { return nil }
            guard "mlhvcsqtaz".utf8.contains(bytes[index] | 0x20) else { return nil }
            index += 1
            return bytes[index - 1]
        }

        /// An arc's flag: a single 0 or 1, which may touch what follows.
        mutating func flag() -> Bool? {
            skipSeparators()
            guard index < bytes.count, bytes[index] == UInt8(ascii: "0") || bytes[index] == UInt8(ascii: "1") else {
                return nil
            }
            index += 1
            return bytes[index - 1] == UInt8(ascii: "1")
        }

        /// A number: a sign, digits with at most one point, and an exponent.
        /// A second point or a sign starts the next number.
        mutating func number() -> CGFloat? {
            skipSeparators()
            let begin = index
            if index < bytes.count, bytes[index] == UInt8(ascii: "-") || bytes[index] == UInt8(ascii: "+") { index += 1 }
            var digits = 0, point = false
            while index < bytes.count {
                let byte = bytes[index]
                if byte >= UInt8(ascii: "0"), byte <= UInt8(ascii: "9") {
                    digits += 1
                } else if byte == UInt8(ascii: "."), !point {
                    point = true
                } else {
                    break
                }
                index += 1
            }
            guard digits > 0 else { index = begin; return nil }
            if index < bytes.count, bytes[index] == UInt8(ascii: "e") || bytes[index] == UInt8(ascii: "E") {
                let mark = index
                index += 1
                if index < bytes.count, bytes[index] == UInt8(ascii: "-") || bytes[index] == UInt8(ascii: "+") { index += 1 }
                let exponentStart = index
                while index < bytes.count, bytes[index] >= UInt8(ascii: "0"), bytes[index] <= UInt8(ascii: "9") { index += 1 }
                if index == exponentStart { index = mark }
            }
            guard let text = String(bytes: bytes[begin..<index], encoding: .ascii), let value = Double(text),
                  value.isFinite else { index = begin; return nil }
            return CGFloat(value)
        }
    }
}

/// Codex's mark: its cloud with a prompt, `>_`, cut out of it, in the
/// violet to blue of Codex's app icon. The path is from
/// @lobehub/icons-static-svg (MIT), drawn on a 24-point square; the mark
/// belongs to OpenAI and only names whose session it is. It ships with the
/// app because Codex runs from a terminal as often as from its app, and a
/// session should read as Codex's either way.
enum NotchCodexMark {
    static let side: CGFloat = 24

    static let pathData = "M8.086.457a6.105 6.105 0 013.046-.415c1.333.153 2.521.72 3.564 1.7a.117.117 0 00.107.029c1.408-.346 2.762-.224 4.061.366l.063.03.154.076c1.357.703 2.33 1.77 2.918 3.198.278.679.418 1.388.421 2.126a5.655 5.655 0 01-.18 1.631.167.167 0 00.04.155 5.982 5.982 0 011.578 2.891c.385 1.901-.01 3.615-1.183 5.14l-.182.22a6.063 6.063 0 01-2.934 1.851.162.162 0 00-.108.102c-.255.736-.511 1.364-.987 1.992-1.199 1.582-2.962 2.462-4.948 2.451-1.583-.008-2.986-.587-4.21-1.736a.145.145 0 00-.14-.032c-.518.167-1.04.191-1.604.185a5.924 5.924 0 01-2.595-.622 6.058 6.058 0 01-2.146-1.781c-.203-.269-.404-.522-.551-.821a7.74 7.74 0 01-.495-1.283 6.11 6.11 0 01-.017-3.064.166.166 0 00.008-.074.115.115 0 00-.037-.064 5.958 5.958 0 01-1.38-2.202 5.196 5.196 0 01-.333-1.589 6.915 6.915 0 01.188-2.132c.45-1.484 1.309-2.648 2.577-3.493.282-.188.55-.334.802-.438.286-.12.573-.22.861-.304a.129.129 0 00.087-.087A6.016 6.016 0 015.635 2.31C6.315 1.464 7.132.846 8.086.457zm-.804 7.85a.848.848 0 00-1.473.842l1.694 2.965-1.688 2.848a.849.849 0 001.46.864l1.94-3.272a.849.849 0 00.007-.854l-1.94-3.393zm5.446 6.24a.849.849 0 000 1.695h4.848a.849.849 0 000-1.696h-4.848z"

    /// The gradient down the mark, top to bottom: #B1A7FF, #7A9DFF, #3941FF.
    static let stops: [(location: CGFloat, red: CGFloat, green: CGFloat, blue: CGFloat)] = [
        (0, 0xB1 / 255, 0xA7 / 255, 0xFF / 255),
        (0.5, 0x7A / 255, 0x9D / 255, 0xFF / 255),
        (1, 0x39 / 255, 0x41 / 255, 0xFF / 255),
    ]

    /// The mark on its 24-point square, y pointing down.
    static let path: CGPath = NotchSVGPath.path(pathData) ?? CGMutablePath()

    /// Draws the mark filling `rect` in a context whose y points down, as a
    /// flipped view's or image's does. The prompt's two strokes are holes.
    static func draw(in context: CGContext, rect: CGRect) {
        guard rect.width > 0, rect.height > 0,
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let gradient = CGGradient(colorSpace: space,
                                        colorComponents: stops.flatMap { [$0.red, $0.green, $0.blue, 1] },
                                        locations: stops.map(\.location), count: stops.count)
        else { return }
        let scale = min(rect.width, rect.height) / side
        context.saveGState()
        context.translateBy(x: rect.midX - side * scale / 2, y: rect.midY - side * scale / 2)
        context.scaleBy(x: scale, y: scale)
        context.addPath(path)
        context.clip(using: .evenOdd)
        context.drawLinearGradient(gradient, start: CGPoint(x: side / 2, y: 0), end: CGPoint(x: side / 2, y: side),
                                   options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
        context.restoreGState()
    }
}
