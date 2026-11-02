// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The marks of Gemini CLI, Copilot CLI, OpenCode and Cursor, drawn by the
/// app so a session reads as its agent's whether that agent's app is on the
/// Mac or not: Gemini's four-pointed star, Copilot's goggled face, OpenCode's
/// hollow square and Cursor's cube. Each fills the same square, in the
/// agent's color, and breathes while it works, unless Reduce Motion is on.
struct NotchHookAgentMark: View {
    let provider: ClaudeSessionProvider
    var size: CGFloat = 13
    var working = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var breathing = false

    var body: some View {
        let animates = working && !reduceMotion
        Canvas { context, canvas in
            let rect = CGRect(origin: .zero, size: canvas).insetBy(dx: canvas.width * 0.08, dy: canvas.height * 0.08)
            Self.draw(provider, in: rect, context: &context)
        }
        .frame(width: size, height: size)
        .scaleEffect(animates && breathing ? 1.08 : 1)
        .opacity(animates && breathing ? 0.82 : 1)
        .animation(animates ? .easeInOut(duration: 0.9).repeatForever(autoreverses: true) : .default, value: breathing)
        .onAppear { breathing = animates }
        .onChange(of: animates) { _, now in breathing = now }
        .accessibilityHidden(true)
        .allowsHitTesting(false)
    }

    static func draw(_ provider: ClaudeSessionProvider, in rect: CGRect, context: inout GraphicsContext) {
        let tint = provider.tint
        switch provider {
        case .gemini:
            context.fill(star(in: rect), with: .linearGradient(
                Gradient(colors: [Color(red: 0.30, green: 0.62, blue: 1.0), Color(red: 0.62, green: 0.50, blue: 0.96),
                                  Color(red: 0.86, green: 0.48, blue: 0.62)]),
                startPoint: CGPoint(x: rect.minX, y: rect.maxY), endPoint: CGPoint(x: rect.maxX, y: rect.minY)))
        case .copilot:
            var face = Path(roundedRect: rect.insetBy(dx: 0, dy: rect.height * 0.12), cornerRadius: rect.width * 0.32)
            let eye = CGSize(width: rect.width * 0.26, height: rect.height * 0.30)
            for x in [rect.minX + rect.width * 0.18, rect.maxX - rect.width * 0.18 - eye.width] {
                face.addRoundedRect(in: CGRect(origin: CGPoint(x: x, y: rect.midY - eye.height * 0.7), size: eye),
                                    cornerSize: CGSize(width: eye.width * 0.45, height: eye.height * 0.45))
            }
            context.fill(face, with: .color(tint), style: FillStyle(eoFill: true))
        case .opencode:
            var block = Path(CGRect(x: rect.minX + rect.width * 0.1, y: rect.minY, width: rect.width * 0.8, height: rect.height))
            block.addRect(CGRect(x: rect.minX + rect.width * 0.32, y: rect.minY + rect.height * 0.22,
                                 width: rect.width * 0.36, height: rect.height * 0.56))
            context.fill(block, with: .color(tint), style: FillStyle(eoFill: true))
        case .cursor:
            let center = CGPoint(x: rect.midX, y: rect.midY)
            let radius = min(rect.width, rect.height) / 2
            let corners = (0..<6).map { index -> CGPoint in
                let angle = Double(index) * .pi / 3 - .pi / 2
                return CGPoint(x: center.x + radius * CGFloat(cos(angle)), y: center.y + radius * CGFloat(sin(angle)))
            }
            var cube = Path()
            cube.addLines(corners)
            cube.closeSubpath()
            context.fill(cube, with: .color(tint.opacity(0.55)))
            // The lit face, from the top corner down to the middle and round the left.
            var lit = Path()
            lit.addLines([corners[0], center, corners[3], corners[4], corners[5]])
            lit.closeSubpath()
            context.fill(lit, with: .color(tint))
        case .claude, .codex:
            break
        }
    }

    /// Gemini's star: four points joined by inward curves.
    static func star(in rect: CGRect) -> Path {
        let c = CGPoint(x: rect.midX, y: rect.midY)
        var path = Path()
        path.move(to: CGPoint(x: c.x, y: rect.minY))
        path.addQuadCurve(to: CGPoint(x: rect.maxX, y: c.y), control: c)
        path.addQuadCurve(to: CGPoint(x: c.x, y: rect.maxY), control: c)
        path.addQuadCurve(to: CGPoint(x: rect.minX, y: c.y), control: c)
        path.addQuadCurve(to: CGPoint(x: c.x, y: rect.minY), control: c)
        path.closeSubpath()
        return path
    }
}
