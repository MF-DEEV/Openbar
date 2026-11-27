// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import CoreGraphics
import Foundation

/// Decisions for the sessions' own menu bar item and the panel under it
/// (#331), kept out of AppKit so they can be tested.
enum SessionsMenuBarSupport {
    /// The panel: the island's Sessions page with its top row, a size that
    /// shows six rows, or a card with its command and the reply field.
    static let panelSize = CGSize(width: 460, height: 440)
    /// Enlarged on a session's page, to read and write in comfortably.
    static let enlargedSize = CGSize(width: 720, height: 640)
    /// Room kept below the menu bar and from the screen's edges.
    static let gap: CGFloat = 6
    static let margin: CGFloat = 8
    static let cornerRadius: CGFloat = 20
    /// The page inside, as the island lays out a page with a top row.
    static let horizontalInset: CGFloat = 18
    static let headerTopInset: CGFloat = 12

    /// The page's own size inside a panel of this size.
    static func contentSize(in panel: CGSize) -> CGSize {
        CGSize(width: max(0, panel.width - horizontalInset * 2),
               height: max(0, panel.height - headerTopInset - NotchLayout.headerHeight - NotchLayout.spacing
                   - NotchLayout.bottomInset))
    }

    /// Where the panel goes, in screen coordinates (y up): centered under
    /// the item, a few points below the menu bar, inside the screen's
    /// visible part. Without a frame to trust for the item, as when the
    /// notch hides it or macOS has not placed it yet, it hangs at the right
    /// end of the menu bar, where status items live.
    static func frame(size: CGSize, under item: CGRect?, visible: CGRect) -> CGRect {
        let width = max(0, min(size.width, visible.width - margin * 2))
        let top = (item?.minY ?? visible.maxY) - gap
        let height = max(0, min(size.height, top - visible.minY - margin))
        let midX = item?.midX ?? (visible.maxX - margin - width / 2)
        let x = min(max(midX - width / 2, visible.minX + margin), visible.maxX - margin - width)
        return CGRect(x: x, y: top - height, width: width, height: height)
    }

    /// The count beside the face: nothing for none or one ask, which the
    /// dot already tells, the number from two.
    static func countTitle(_ waiting: Int) -> String { waiting >= 2 ? "\(waiting)" : "" }

    /// One waiting ask as the menu lists it.
    struct MenuAsk: Equatable {
        let session: String
        let request: String
        /// The session's name, then what the ask is about.
        let title: String
        /// Allow and Deny in the menu exactly where the island's queue
        /// offers them; a question, a plan or a command to read whole only
        /// opens, on its card.
        let decides: Bool
        /// Why the island would not let Allow all take it, when it looks risky.
        let risk: PermissionRisk?
    }

    /// Every waiting ask, oldest first, as the queue lists them.
    static func menuAsks(_ sessions: [ClaudeSession], name: (ClaudeSession) -> String) -> [MenuAsk] {
        NotchPermissionQueue(sessions).rows.map { row in
            let line = row.line.isEmpty ? row.ask.tool : row.line
            return MenuAsk(session: row.session.id, request: row.ask.requestID,
                           title: [name(row.session), line].filter { !$0.isEmpty }.joined(separator: " — "),
                           decides: row.answersHere,
                           risk: row.ask.risk.isSafe || row.ask.risk == .unassessed ? nil : row.ask.risk)
        }
    }

    /// A menu title kept to one readable line.
    static func menuTitle(_ title: String, limit: Int = 72) -> String {
        let line = title.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""
        guard line.count > limit else { return line }
        return String(line.prefix(limit - 1)).trimmingCharacters(in: .whitespaces) + "…"
    }
}
