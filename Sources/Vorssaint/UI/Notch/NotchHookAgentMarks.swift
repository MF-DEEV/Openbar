// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The makers' own marks of Gemini CLI, GitHub Copilot, OpenCode and Cursor,
/// drawn by the app from the same paths the Windows island uses, so a session
/// reads as its agent's whether that agent's app is on the Mac or not. Paths
/// from @lobehub/icons-static-svg (MIT); the marks belong to Google, GitHub,
/// OpenCode and Anysphere and only name whose session it is. Gemini CLI's
/// keeps its gradient; the other three are one color, as their makers show
/// them on dark. Each breathes while its session works, unless Reduce Motion
/// is on.
struct NotchHookAgentMark: View {
    let provider: ClaudeSessionProvider
    var size: CGFloat = 13
    var working = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var breathing = false

    var body: some View {
        let animates = working && !reduceMotion
        Canvas { context, canvas in
            Self.draw(provider, in: CGRect(origin: .zero, size: canvas), context: &context)
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
        guard let mark = NotchHookAgentMarkPaths.mark(provider), rect.width > 0, rect.height > 0 else { return }
        let scale = min(rect.width, rect.height) / NotchHookAgentMarkPaths.side
        let transform = CGAffineTransform(translationX: rect.midX - NotchHookAgentMarkPaths.side * scale / 2,
                                          y: rect.midY - NotchHookAgentMarkPaths.side * scale / 2)
            .scaledBy(x: scale, y: scale)
        for layer in mark.layers {
            let path = Path(layer.path).applying(transform)
            switch layer.fill {
            case .solid(let color):
                context.fill(path, with: .color(color), style: FillStyle(eoFill: true))
            case .gradient(let colors, let start, let end):
                context.fill(path, with: .linearGradient(Gradient(stops: colors.map { Gradient.Stop(color: $0.color, location: $0.location) }),
                                                         startPoint: start.applying(transform), endPoint: end.applying(transform)),
                             style: FillStyle(eoFill: true))
            }
        }
    }
}

/// The marks' paths on their 24-point square, y pointing down.
enum NotchHookAgentMarkPaths {
    static let side: CGFloat = 24

    enum Fill {
        case solid(Color)
        case gradient([(color: Color, location: CGFloat)], start: CGPoint, end: CGPoint)
    }

    struct Layer {
        let path: CGPath
        let fill: Fill
    }

    struct Mark {
        let layers: [Layer]
    }

    /// How the three one-color marks show on the island's black.
    static let light = Color(white: 0.94)

    /// Gemini CLI's rounded square in Gemini's red, violet and blue, with the
    /// prompt cut into its dark face.
    static let geminiSquare = "M0 4.391A4.391 4.391 0 014.391 0h15.217A4.391 4.391 0 0124 4.391v15.217A4.391 4.391 0 0119.608 24H4.391A4.391 4.391 0 010 19.608V4.391z"
    static let geminiFace = "M19.74 1.444a2.816 2.816 0 012.816 2.816v15.48a2.816 2.816 0 01-2.816 2.816H4.26a2.816 2.816 0 01-2.816-2.816V4.26A2.816 2.816 0 014.26 1.444h15.48zM7.236 8.564l7.752 3.728-7.752 3.727v2.802l9.557-4.596v-3.866L7.236 5.763v2.801z"
    static let copilot = "M19.245 5.364c1.322 1.36 1.877 3.216 2.11 5.817.622 0 1.2.135 1.592.654l.73.964c.21.278.323.61.323.955v2.62c0 .339-.173.669-.453.868C20.239 19.602 16.157 21.5 12 21.5c-4.6 0-9.205-2.583-11.547-4.258-.28-.2-.452-.53-.453-.868v-2.62c0-.345.113-.679.321-.956l.73-.963c.392-.517.974-.654 1.593-.654l.029-.297c.25-2.446.81-4.213 2.082-5.52 2.461-2.54 5.71-2.851 7.146-2.864h.198c1.436.013 4.685.323 7.146 2.864zm-7.244 4.328c-.284 0-.613.016-.962.05-.123.447-.305.85-.57 1.108-1.05 1.023-2.316 1.18-2.994 1.18-.638 0-1.306-.13-1.851-.464-.516.165-1.012.403-1.044.996a65.882 65.882 0 00-.063 2.884l-.002.48c-.002.563-.005 1.126-.013 1.69.002.326.204.63.51.765 2.482 1.102 4.83 1.657 6.99 1.657 2.156 0 4.504-.555 6.985-1.657a.854.854 0 00.51-.766c.03-1.682.006-3.372-.076-5.053-.031-.596-.528-.83-1.046-.996-.546.333-1.212.464-1.85.464-.677 0-1.942-.157-2.993-1.18-.266-.258-.447-.661-.57-1.108-.32-.032-.64-.049-.96-.05zm-2.525 4.013c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zm5 0c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zM7.635 5.087c-1.05.102-1.935.438-2.385.906-.975 1.037-.765 3.668-.21 4.224.405.394 1.17.657 1.995.657h.09c.649-.013 1.785-.176 2.73-1.11.435-.41.705-1.433.675-2.47-.03-.834-.27-1.52-.63-1.813-.39-.336-1.275-.482-2.265-.394zm6.465.394c-.36.292-.6.98-.63 1.813-.03 1.037.24 2.06.675 2.47.968.957 2.136 1.104 2.776 1.11h.044c.825 0 1.59-.263 1.995-.657.555-.556.765-3.187-.21-4.224-.45-.468-1.335-.804-2.385-.906-.99-.088-1.875.058-2.265.394zM12 7.615c-.24 0-.525.015-.84.044.03.16.045.336.06.526l-.001.159a2.94 2.94 0 01-.014.25c.225-.022.425-.027.612-.028h.366c.187 0 .387.006.612.028-.015-.146-.015-.277-.015-.409.015-.19.03-.365.06-.526a9.29 9.29 0 00-.84-.044z"
    static let opencode = "M16 6H8v12h8V6zm4 16H4V2h16v20z"
    static let cursor = "M22.106 5.68L12.5.135a.998.998 0 00-.998 0L1.893 5.68a.84.84 0 00-.419.726v11.186c0 .3.16.577.42.727l9.607 5.547a.999.999 0 00.998 0l9.608-5.547a.84.84 0 00.42-.727V6.407a.84.84 0 00-.42-.726zm-.603 1.176L12.228 22.92c-.063.108-.228.064-.228-.061V12.34a.59.59 0 00-.295-.51l-9.11-5.26c-.107-.062-.063-.228.062-.228h18.55c.264 0 .428.286.296.514z"

    private static func path(_ data: String) -> CGPath { NotchSVGPath.path(data) ?? CGMutablePath() }

    private static let marks: [ClaudeSessionProvider: Mark] = [
        .gemini: Mark(layers: [
            Layer(path: path(geminiSquare), fill: .gradient([(Color(red: 0xEE / 255, green: 0x4D / 255, blue: 0x5D / 255), 0),
                                                             (Color(red: 0xB3 / 255, green: 0x81 / 255, blue: 0xDD / 255), 0.328),
                                                             (Color(red: 0x20 / 255, green: 0x7C / 255, blue: 0xFE / 255), 0.476)],
                                                            start: CGPoint(x: 24, y: 6.587), end: CGPoint(x: 0, y: 16.494))),
            Layer(path: path(geminiFace), fill: .solid(Color(red: 0x1E / 255, green: 0x1E / 255, blue: 0x2E / 255))),
        ]),
        .copilot: Mark(layers: [Layer(path: path(copilot), fill: .solid(light))]),
        .opencode: Mark(layers: [Layer(path: path(opencode), fill: .solid(light))]),
        .cursor: Mark(layers: [Layer(path: path(cursor), fill: .solid(light))]),
    ]

    static func mark(_ provider: ClaudeSessionProvider) -> Mark? { marks[provider] }
}
