// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

/// What Claude wrote, laid out by AppKit so its links act as links: blue,
/// a pointing hand over them and opened at a click, while the rest stays
/// selectable. The island's panel never activates the app, so the cursor is
/// set from the view's own tracking rather than cursor rectangles, which
/// only an active app's key window gets.
struct NotchClaudeRichText: NSViewRepresentable {
    let text: AttributedString
    let size: CGFloat
    var color: NSColor = .white.withAlphaComponent(0.88)
    var lineSpacing: CGFloat = 0
    /// Lines shown at most; none for all of them.
    var maxLines = 0

    static let linkColor = NSColor(red: 0.42, green: 0.66, blue: 1.0, alpha: 1)

    func makeNSView(context: Context) -> NotchLinkTextView {
        let view = NotchLinkTextView(usingTextLayoutManager: false)
        view.isEditable = false
        view.isSelectable = true
        view.drawsBackground = false
        view.textContainerInset = .zero
        view.textContainer?.lineFragmentPadding = 0
        view.textContainer?.widthTracksTextView = true
        view.isVerticallyResizable = false
        view.isHorizontallyResizable = false
        view.linkTextAttributes = [.foregroundColor: Self.linkColor, .cursor: NSCursor.pointingHand]
        view.setContentHuggingPriority(.defaultLow, for: .horizontal)
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return view
    }

    func updateNSView(_ view: NotchLinkTextView, context: Context) {
        let content = Self.attributed(text, size: size, color: color, lineSpacing: lineSpacing)
        if view.textStorage?.isEqual(to: content) != true { view.textStorage?.setAttributedString(content) }
        view.textContainer?.maximumNumberOfLines = maxLines
        view.textContainer?.lineBreakMode = maxLines > 0 ? .byTruncatingTail : .byWordWrapping
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView view: NotchLinkTextView, context: Context) -> CGSize? {
        guard let container = view.textContainer, let layout = view.layoutManager else { return nil }
        let width = max(1, proposal.width ?? 400)
        container.containerSize = CGSize(width: width, height: .greatestFiniteMagnitude)
        layout.ensureLayout(for: container)
        return CGSize(width: width, height: ceil(layout.usedRect(for: container).height))
    }

    /// The Markdown's runs as AppKit text: bold, italics and code in their
    /// fonts, links kept for the view to color and open.
    static func attributed(_ text: AttributedString, size: CGFloat, color: NSColor, lineSpacing: CGFloat) -> NSAttributedString {
        let result = NSMutableAttributedString()
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = lineSpacing
        for run in text.runs {
            let intent = run.inlinePresentationIntent ?? []
            var font = intent.contains(.code) ? NSFont.monospacedSystemFont(ofSize: size - 1, weight: .regular)
                : NSFont.systemFont(ofSize: size)
            var traits: NSFontDescriptor.SymbolicTraits = []
            if intent.contains(.stronglyEmphasized) { traits.insert(.bold) }
            if intent.contains(.emphasized) { traits.insert(.italic) }
            if !traits.isEmpty {
                font = NSFont(descriptor: font.fontDescriptor.withSymbolicTraits(traits), size: font.pointSize) ?? font
            }
            var attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color, .paragraphStyle: paragraph]
            if intent.contains(.strikethrough) { attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
            if let link = run.link { attributes[.link] = link }
            result.append(NSAttributedString(string: String(text[run.range].characters), attributes: attributes))
        }
        return result
    }
}

/// A text view that shows a pointing hand over its links even in a panel
/// that never becomes the active app's.
final class NotchLinkTextView: NSTextView {
    private var tracking: NSTrackingArea?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: bounds, options: [.activeAlways, .mouseMoved, .mouseEnteredAndExited, .inVisibleRect],
                                  owner: self, userInfo: nil)
        addTrackingArea(area)
        tracking = area
    }

    override func mouseMoved(with event: NSEvent) {
        (link(at: convert(event.locationInWindow, from: nil)) == nil ? NSCursor.iBeam : NSCursor.pointingHand).set()
    }

    override func mouseExited(with event: NSEvent) {
        NSCursor.arrow.set()
    }

    override func mouseDown(with event: NSEvent) {
        // A link opens at once; the panel would otherwise take the first
        // click only to come forward.
        if let url = link(at: convert(event.locationInWindow, from: nil)) {
            NSWorkspace.shared.open(url)
            return
        }
        super.mouseDown(with: event)
    }

    /// The link under a point, only where a glyph of it actually is.
    func link(at point: NSPoint) -> URL? {
        guard let layout = layoutManager, let container = textContainer, let storage = textStorage, storage.length > 0 else {
            return nil
        }
        var fraction: CGFloat = 0
        let glyph = layout.glyphIndex(for: point, in: container, fractionOfDistanceThroughGlyph: &fraction)
        guard layout.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: container).contains(point) else {
            return nil
        }
        let index = layout.characterIndexForGlyph(at: glyph)
        guard index < storage.length else { return nil }
        let value = storage.attribute(.link, at: index, effectiveRange: nil)
        return (value as? URL) ?? (value as? String).flatMap(URL.init(string:))
    }
}
