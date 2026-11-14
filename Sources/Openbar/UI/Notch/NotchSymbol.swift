// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

/// Marks of Openbar's own that stand where an SF Symbol name goes. A module's
/// `symbol` is a plain string read by many views; these names are drawn here
/// instead of being looked up in SF Symbols, which has nothing for them.
enum OpenbarSymbol {
    /// The Sessions mark: an agent's face with Clawd's two tall eyes.
    static let agentFace = "openbar.agent.face"

    /// The SF Symbol whose box a drawn mark takes, so it sits at the size and
    /// on the baseline the symbol it replaced did.
    static func standIn(for name: String) -> String {
        name == agentFace ? "rectangle.stack" : name
    }

    /// An AppKit image for menus and other places that take an NSImage: the
    /// drawn mark as a template, or the SF Symbol itself.
    static func nsImage(_ name: String, accessibilityDescription: String? = nil) -> NSImage? {
        guard name == agentFace else {
            return NSImage(systemSymbolName: name, accessibilityDescription: accessibilityDescription)
        }
        let size = NSImage(systemSymbolName: standIn(for: name), accessibilityDescription: nil)?.size
            ?? NSSize(width: 18, height: 14)
        let image = NSImage(size: size, flipped: true) { rect in
            NSColor.black.setFill()
            let face = NSBezierPath()
            face.windingRule = .evenOdd
            for part in AgentFaceShape.parts(in: rect) {
                face.append(NSBezierPath(roundedRect: part.rect, xRadius: part.radius, yRadius: part.radius))
            }
            face.fill()
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = accessibilityDescription
        return image
    }

    /// A SwiftUI image for menus, which draw only images, not shapes.
    static func menuImage(_ name: String) -> Image {
        if name == agentFace, let image = nsImage(name) { return Image(nsImage: image) }
        return Image(systemName: name)
    }
}

/// `Image(systemName:)` that also draws Openbar's own marks. It takes the
/// font, weight, scale and foreground style the way a symbol does.
struct SymbolImage: View {
    let name: String

    init(_ name: String) { self.name = name }

    var body: some View {
        if name == OpenbarSymbol.agentFace {
            Image(systemName: OpenbarSymbol.standIn(for: name))
                .hidden()
                .overlay { AgentFaceShape().fill(.foreground, style: FillStyle(eoFill: true)) }
                .accessibilityHidden(true)
        } else {
            Image(systemName: name)
        }
    }
}

/// A rounded face 18 by 15 with two tall eyes cut out of it, fitted into the
/// rectangle it is given.
struct AgentFaceShape: Shape {
    struct Part { let rect: CGRect; let radius: CGFloat }

    static func parts(in rect: CGRect) -> [Part] {
        // Filled, it reads bigger than the outline it stands beside; a little
        // inside the box matches the weight of the symbols around it.
        let aspect: CGFloat = 18 / 15
        let fill: CGFloat = 0.86
        var width = rect.width * fill
        var height = width / aspect
        if height > rect.height * fill { height = rect.height * fill; width = height * aspect }
        guard width > 0, height > 0 else { return [] }
        let face = CGRect(x: rect.midX - width / 2, y: rect.midY - height / 2, width: width, height: height)
        let unit = width / 18
        let eyes = [5.0, 10.6].map { x in
            Part(rect: CGRect(x: face.minX + x * unit, y: face.minY + 4.5 * unit,
                              width: 2.4 * unit, height: 5.5 * unit),
                 radius: 1.2 * unit)
        }
        return [Part(rect: face, radius: 4.5 * unit)] + eyes
    }

    func path(in rect: CGRect) -> Path {
        var path = Path()
        for part in Self.parts(in: rect) {
            path.addRoundedRect(in: part.rect, cornerSize: CGSize(width: part.radius, height: part.radius),
                                style: .continuous)
        }
        return path
    }
}
