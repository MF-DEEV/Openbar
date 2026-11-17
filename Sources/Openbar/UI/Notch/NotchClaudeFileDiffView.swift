// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// What a file change would do, file by file: removed lines in red, added
/// ones in green, a few unchanged ones around them, and a row to show more
/// of the file where lines are left out. Long diffs scroll.
struct NotchClaudeFileDiffView: View {
    let diffs: [ClaudeFileDiff]
    let text: NotchClaudeSessionStrings
    var maxHeight: CGFloat = 76
    /// How far each gap is open, by file and by the gap's place in its lines.
    /// Only for the diffs on screen: other diffs start closed.
    @State private var opened: [Int: [Int: ClaudeFileDiffs.Opened]] = [:]
    /// One digit of the 10.5 pt monospaced numbers.
    static let digitWidth: CGFloat = 6.6

    static let added = Color(red: 0.36, green: 0.84, blue: 0.52)
    static let removed = Color(red: 1.0, green: 0.42, blue: 0.42)

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            // One file names itself above the lines; several, each above its own.
            if diffs.count == 1, let diff = diffs.first { header(diff) }
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(diffs.enumerated()), id: \.offset) { file, diff in
                        VStack(alignment: .leading, spacing: 0) {
                            if diffs.count > 1 { header(diff).padding(.bottom, 4) }
                            lines(diff, file: file)
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxHeight: maxHeight)
            .fixedSize(horizontal: false, vertical: true)
            .background(.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            // A mask, not clipShape: on a ScrollView clipShape rounds every
            // colored line into a pill as well.
            .mask(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .onChange(of: diffs) { opened = [:] }
    }

    // MARK: Header

    /// The file, shortened with the whole path on hover, what happens to it,
    /// and how many lines come and go.
    private func header(_ diff: ClaudeFileDiff) -> some View {
        HStack(spacing: 6) {
            Image(systemName: diff.change == .add ? "doc.badge.plus" : diff.change == .delete ? "doc.badge.minus" : "doc.text")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.white.opacity(0.6))
            (Text(Self.folder(diff.path)).foregroundColor(.white.opacity(0.5))
                + Text(Self.name(diff.path)).foregroundColor(.white.opacity(0.95)))
                .font(.system(size: 11, weight: .semibold, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.head)
                .help(diff.path)
                .layoutPriority(1)
            // On the path's line, so the buttons below stay in sight.
            if let note = note(diff) {
                Text(note)
                    .font(.system(size: 10))
                    .foregroundStyle(.white.opacity(0.55))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .layoutPriority(2)
            }
            Spacer(minLength: 4)
            counts(diff)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel([diff.path, note(diff), text.diffCounts(added: diff.added, removed: diff.removed)]
            .compactMap { $0 }.joined(separator: ", "))
    }

    private func counts(_ diff: ClaudeFileDiff) -> some View {
        HStack(spacing: 5) {
            Text("+\(diff.added)").foregroundStyle(Self.added)
            Text("−\(diff.removed)").foregroundStyle(Self.removed)
        }
        .font(.system(size: 10.5, weight: .semibold, design: .monospaced))
        .fixedSize()
    }

    private func note(_ diff: ClaudeFileDiff) -> String? {
        var parts: [String] = []
        switch diff.change {
        case .add: parts.append(diff.cell != nil ? text.diffNewCell : text.diffNewFile)
        case .delete: parts.append(diff.cell != nil ? text.diffDeletedCell : text.diffDeletedFile)
        case .replace: parts.append(text.diffReplacesFile)
        case .update: if let cell = diff.cell, !cell.isEmpty { parts.append(text.diffCell(cell)) }
        }
        if let places = diff.places { parts.append(text.diffPlaces(places)) }
        if diff.everyMatch { parts.append(text.diffEveryMatch) }
        if let moved = diff.movedTo { parts.append(text.diffMovesTo((moved as NSString).abbreviatingWithTildeInPath)) }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    // MARK: Lines

    private func lines(_ diff: ClaudeFileDiff, file: Int) -> some View {
        let open = opened[file] ?? [:]
        let rows = ClaudeFileDiffs.rows(diff.lines, opened: open)
        let shown = rows.compactMap { row -> Int? in
            if case let .line(line, _) = row { return line.number }
            return nil
        }
        let digits = String(shown.max() ?? 0).count
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(rows, id: \.key) { row in
                switch row {
                case let .line(line, key):
                    self.row(line, digits: digits)
                        // Lines a click brings in fade in where they land.
                        .modifier(NotchClaudeDiffReveal(active: key.contains("h") || key.contains("t")))
                case let .gap(index, edge, hidden, down, up, all):
                    if edge == nil || !(down || up || all) {
                        gapMark(digits: digits)
                    } else if let gap = diff.lines[index].gap {
                        NotchClaudeDiffGapRow(hidden: hidden, down: down, up: up, all: all,
                                              edge: edge ?? .middle, digits: digits, text: text) { way in
                            opened[file, default: [:]][index] = ClaudeFileDiffs.open(gap, open[index] ?? .init(), way)
                        }
                    }
                }
            }
            if diff.hidden > 0 {
                Text(text.diffMoreLines(diff.hidden))
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.white.opacity(0.5))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
            }
        }
        .padding(.vertical, 4)
    }

    /// Lines left out with nothing kept to show in their place.
    private func gapMark(digits: Int) -> some View {
        Text("⋯")
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.white.opacity(0.35))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, digits > 0 ? CGFloat(digits) * Self.digitWidth + 12 : 8)
            .padding(.vertical, 1)
            .accessibilityHidden(true)
    }

    @ViewBuilder private func row(_ line: ClaudeFileDiff.Line, digits: Int) -> some View {
        if line.kind == .gap {
            gapMark(digits: digits)
        } else {
            HStack(alignment: .firstTextBaseline, spacing: 0) {
                if digits > 0 {
                    Text(line.number.map(String.init) ?? "")
                        .foregroundStyle(.white.opacity(0.35))
                        .lineLimit(1)
                        .fixedSize()
                        .frame(minWidth: CGFloat(digits) * Self.digitWidth, alignment: .trailing)
                        .padding(.trailing, 6)
                }
                Text(sign(line.kind))
                    .foregroundStyle(tint(line.kind))
                    .frame(width: 10, alignment: .leading)
                Text(line.text.isEmpty ? " " : line.text)
                    .foregroundStyle(line.kind == .context ? .white.opacity(0.6) : .white.opacity(0.95))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .font(.system(size: 10.5, design: .monospaced))
            .padding(.leading, 6)
            .padding(.trailing, 8)
            .padding(.vertical, 0.5)
            .background(background(line.kind), in: Rectangle())
            .textSelection(.enabled)
            .accessibilityElement(children: .combine)
            .accessibilityLabel(text.diffLineLabel(added: line.kind == .added, removed: line.kind == .removed,
                                                   number: line.number, text: line.text))
        }
    }

    private func sign(_ kind: ClaudeFileDiff.Line.Kind) -> String {
        switch kind {
        case .added: return "+"
        case .removed: return "−"
        default: return " "
        }
    }

    private func tint(_ kind: ClaudeFileDiff.Line.Kind) -> Color {
        switch kind {
        case .added: return Self.added
        case .removed: return Self.removed
        default: return .clear
        }
    }

    private func background(_ kind: ClaudeFileDiff.Line.Kind) -> Color {
        switch kind {
        case .added: return Self.added.opacity(0.14)
        case .removed: return Self.removed.opacity(0.14)
        default: return .clear
        }
    }

    // MARK: Paths

    /// The folder, from the home folder on, with its trailing slash.
    static func folder(_ path: String) -> String {
        let folder = ((path as NSString).deletingLastPathComponent as NSString).abbreviatingWithTildeInPath
        return folder.isEmpty ? "" : folder.hasSuffix("/") ? folder : folder + "/"
    }

    static func name(_ path: String) -> String { (path as NSString).lastPathComponent }
}

private extension ClaudeFileDiffs.Row {
    var key: String {
        switch self {
        case let .line(_, key): return key
        case let .gap(index, _, _, _, _, _): return "gap\(index)"
        }
    }
}

/// A row in place of lines a diff leaves out: arrows that show the next
/// lines from either side, or all of them when few are left.
struct NotchClaudeDiffGapRow: View {
    let hidden: Int
    let down: Bool
    let up: Bool
    let all: Bool
    let edge: ClaudeFileDiff.Gap.Edge
    let digits: Int
    let text: NotchClaudeSessionStrings
    let open: (ClaudeFileDiffs.Way) -> Void

    var body: some View {
        HStack(spacing: 2) {
            if all {
                arrow(edge == .top ? "arrow.up" : edge == .bottom ? "arrow.down" : "arrow.up.and.down", .all,
                      help: text.diffShowAll(hidden))
            } else {
                if down { arrow("arrow.down", .down, help: text.diffShowBelow) }
                if up { arrow("arrow.up", .up, help: text.diffShowAbove) }
            }
            Text(text.diffHiddenLines(hidden))
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.white.opacity(0.5))
                .lineLimit(1)
                .padding(.leading, 4)
                .accessibilityHidden(true)
            Spacer(minLength: 0)
        }
        .padding(.leading, 4)
        .padding(.vertical, 1)
        .background(.white.opacity(0.05), in: Rectangle())
    }

    private func arrow(_ symbol: String, _ way: ClaudeFileDiffs.Way, help: String) -> some View {
        Button { open(way) } label: {
            Image(systemName: symbol)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.white.opacity(0.75))
                .frame(width: 22, height: 15)
                .contentShape(Rectangle())
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 4, lifts: false))
        .help(help)
        .accessibilityLabel(help)
    }
}

/// A line a click brought in comes up from faint, once.
private struct NotchClaudeDiffReveal: ViewModifier {
    let active: Bool
    @State private var shown = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        content
            .opacity(!active || shown || reduceMotion ? 1 : 0.25)
            .onAppear {
                guard active, !shown else { return }
                // Opacity only: animating layout inside the island's scroll
                // view draws it shifted for a few frames.
                withAnimation(.easeOut(duration: 0.22)) { shown = true }
            }
    }
}
