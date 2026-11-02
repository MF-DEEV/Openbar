// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// What a file change would do, file by file: removed lines in red, added
/// ones in green, a few unchanged ones around them. Long diffs scroll.
struct NotchClaudeFileDiffView: View {
    let diffs: [ClaudeFileDiff]
    let text: NotchClaudeSessionStrings
    var maxHeight: CGFloat = 76
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
                    ForEach(Array(diffs.enumerated()), id: \.offset) { _, diff in
                        VStack(alignment: .leading, spacing: 0) {
                            if diffs.count > 1 { header(diff).padding(.bottom, 4) }
                            lines(diff)
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

    private func lines(_ diff: ClaudeFileDiff) -> some View {
        let digits = String(diff.lines.compactMap(\.number).max() ?? 0).count
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(diff.lines.enumerated()), id: \.offset) { _, line in
                row(line, digits: digits)
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

    @ViewBuilder private func row(_ line: ClaudeFileDiff.Line, digits: Int) -> some View {
        if line.kind == .gap {
            Text("⋯")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.white.opacity(0.35))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, digits > 0 ? CGFloat(digits) * Self.digitWidth + 12 : 8)
                .padding(.vertical, 1)
                .accessibilityHidden(true)
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
