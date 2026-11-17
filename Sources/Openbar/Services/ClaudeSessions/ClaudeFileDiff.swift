// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// What a file change a session asks for would do to one file, line by line,
/// for the person to read before allowing it. Built from the request alone,
/// with the file on disk read only for context; kept in memory with the
/// request and dropped once it is answered, like the rest of the card.
struct ClaudeFileDiff: Equatable {
    enum Change: String, Equatable {
        /// Lines of an existing file change.
        case update
        /// A new file, or a new notebook cell.
        case add
        /// The file, or a notebook cell, goes.
        case delete
        /// The whole file is written over, and what it holds now could not be read.
        case replace
    }

    struct Line: Equatable {
        enum Kind: String, Equatable {
            case context, added, removed
            /// Lines left out between two changes.
            case gap
        }

        let kind: Kind
        let text: String
        /// The line's number in the file: the new one, or the old one for a
        /// removed line. Nil when the file could not be read.
        var number: Int? = nil
        /// What a gap leaves out of the file as read, for the card to show
        /// on a click. Nil for a gap with no file behind it.
        var gap: Gap? = nil
    }

    /// The unchanged lines a gap leaves out, taken from the file read for
    /// context when the request came, so showing them reads nothing.
    struct Gap: Equatable {
        enum Edge: String, Equatable {
            /// Above the first change: opens upward, from the change.
            case top
            /// Between two changes: opens from either side.
            case middle
            /// Below the last change: opens downward, from the change.
            case bottom
        }

        var edge: Edge
        /// How many lines it leaves out.
        var count: Int
        /// The lines kept from its start, right below the lines above it.
        var head: [Line] = []
        /// The lines kept from its end, right above the lines below it.
        var tail: [Line] = []
    }

    var path: String
    var change: Change = .update
    /// How many places a replace-all changes, when the file was read and
    /// holds more than one.
    var places: Int? = nil
    /// A replace-all whose places could not be counted.
    var everyMatch = false
    /// The notebook cell an edit is for; empty for a notebook edit that
    /// names none. Nil for anything but a notebook cell.
    var cell: String? = nil
    /// Where the file moves to.
    var movedTo: String? = nil
    var added = 0
    var removed = 0
    var lines: [Line] = []
    /// Lines past the limit, left out.
    var hidden = 0
}

/// Builds the diffs a permission card shows from what each agent sends.
enum ClaudeFileDiffs {
    /// Unchanged lines shown around each change.
    static let context = 3
    /// Lines kept for one request, across all of its files.
    static let lineLimit = 400
    /// Characters kept of one line.
    static let textLimit = 300
    /// Lines one click on a gap shows.
    static let expandStep = 20
    /// Files larger than this are not read for context.
    static let fileLimit = 2_000_000
    /// Above this many cells the middle of a diff is shown as removed, then
    /// added, instead of being matched line by line.
    static let matchLimit = 4_000_000

    // MARK: Claude Code's tools

    /// The diffs for a Claude Code file tool; empty for any other tool.
    static func diffs(tool: String, input: [String: Any], cwd: String? = nil,
                      read: (String) -> String? = readText) -> [ClaudeFileDiff] {
        func text(_ key: String) -> String? { input[key] as? String }
        let diff: ClaudeFileDiff?
        switch tool {
        case "Edit":
            guard let path = text("file_path"), let old = text("old_string"), let new = text("new_string") else { return [] }
            diff = edits(path: path, [(old, new, input["replace_all"] as? Bool ?? false)], file: read(resolve(path, cwd)))
        case "MultiEdit":
            guard let path = text("file_path"), let list = input["edits"] as? [Any] else { return [] }
            let edits = list.compactMap { value -> (String, String, Bool)? in
                guard let edit = value as? [String: Any], let old = edit["old_string"] as? String,
                      let new = edit["new_string"] as? String else { return nil }
                return (old, new, edit["replace_all"] as? Bool ?? false)
            }
            guard !edits.isEmpty else { return [] }
            diff = self.edits(path: path, edits, file: read(resolve(path, cwd)))
        case "Write":
            guard let path = text("file_path"), let content = text("content") else { return [] }
            let full = resolve(path, cwd)
            diff = write(path: path, content: content, exists: FileManager.default.fileExists(atPath: full), file: read(full))
        case "NotebookEdit":
            guard let path = text("notebook_path") else { return [] }
            diff = notebook(path: path, input: input, file: read(resolve(path, cwd)))
        default:
            diff = nil
        }
        return limited(diff.map { [$0] } ?? [])
    }

    /// Each edit in turn, as Claude Code applies them: later ones see the
    /// file as the earlier ones left it. With the file at hand each shows
    /// where it lands, with lines around it; without, the edit's own text.
    static func edits(path: String, _ edits: [(old: String, new: String, all: Bool)], file: String?) -> ClaudeFileDiff {
        var diff = ClaudeFileDiff(path: path)
        var content = file.map(normalized)
        // The file does not exist yet: an edit with nothing to replace makes it.
        if file == nil, edits.count == 1, edits[0].old.isEmpty { diff.change = .add }
        for (index, edit) in edits.enumerated() {
            let old = normalized(edit.old), new = normalized(edit.new)
            var lines: [ClaudeFileDiff.Line]
            if let current = content, !old.isEmpty, let placed = place(old, new, in: current, all: edit.all) {
                lines = placed.lines
                content = placed.result
                if edit.all, placed.count > 1 { diff.places = (diff.places ?? 0) + placed.count }
            } else {
                // Not in the file as read, or no file: the edit's text alone.
                lines = hunks(match(split(old), split(new)), context: .max, numbered: diff.change == .add)
                content = nil
                if edit.all { diff.everyMatch = true }
            }
            if index > 0, !lines.isEmpty, !diff.lines.isEmpty { diff.lines.append(.init(kind: .gap, text: "")) }
            diff.lines += lines
        }
        // Each edit is read against the file as the one before left it, so
        // the file's ends open only above the first and below the last.
        diff.lines = diff.lines.enumerated().filter { index, line in
            switch line.gap?.edge {
            case .top: return index == 0
            case .bottom: return index == diff.lines.count - 1
            default: return true
            }
        }.map(\.element)
        count(&diff)
        return diff
    }

    /// A whole file written: against what is there now, or all new.
    static func write(path: String, content: String, exists: Bool, file: String?) -> ClaudeFileDiff {
        var diff = ClaudeFileDiff(path: path)
        if let file {
            diff.lines = hunks(match(split(normalized(file)), split(normalized(content))), context: context, numbered: true)
        } else {
            diff.change = exists ? .replace : .add
            diff.lines = hunks(match([], split(normalized(content))), context: context, numbered: true)
        }
        count(&diff)
        return diff
    }

    /// A notebook cell replaced, inserted or deleted.
    static func notebook(path: String, input: [String: Any], file: String?) -> ClaudeFileDiff? {
        let mode = input["edit_mode"] as? String ?? "replace"
        let id = (input["cell_id"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        let source = normalized(input["new_source"] as? String ?? "")
        var diff = ClaudeFileDiff(path: path, cell: id ?? "")
        let old = file.flatMap { cellSource(in: $0, id: id) }.map(normalized)
        switch mode {
        case "insert":
            diff.change = .add
            diff.lines = hunks(match([], split(source)), context: .max, numbered: true)
        case "delete":
            diff.change = .delete
            diff.lines = hunks(match(split(old ?? ""), []), context: .max, numbered: old != nil)
        default:
            diff.lines = hunks(match(split(old ?? ""), split(source)), context: .max, numbered: old != nil)
        }
        count(&diff)
        return diff
    }

    /// A cell's source, found by its id or, as Claude Code numbers cells
    /// without one, by `cell-N`.
    static func cellSource(in notebook: String, id: String?) -> String? {
        guard let id, let data = notebook.data(using: .utf8),
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let cells = json["cells"] as? [[String: Any]] else { return nil }
        var cell = cells.first { $0["id"] as? String == id }
        if cell == nil, id.hasPrefix("cell-"), let index = Int(id.dropFirst(5)), cells.indices.contains(index) {
            cell = cells[index]
        }
        guard let cell else { return nil }
        if let text = cell["source"] as? String { return text }
        return (cell["source"] as? [String])?.joined()
    }

    // MARK: Other agents

    /// Codex's file changes: each file with the patch its app server sends.
    static func codex(_ changes: [Any]) -> [ClaudeFileDiff] {
        limited(changes.compactMap { value -> ClaudeFileDiff? in
            guard let change = value as? [String: Any], let path = change["path"] as? String else { return nil }
            let kind = change["kind"] as? [String: Any]
            let type = kind?["type"] as? String ?? "update"
            let text = change["diff"] as? String ?? ""
            var diff = patch(path: path, text, whole: type == "add" ? .add : type == "delete" ? .delete : nil).first
                ?? ClaudeFileDiff(path: path)
            diff.path = path
            if type == "add" { diff.change = .add }
            if type == "delete" { diff.change = .delete }
            diff.movedTo = (kind?["move_path"] as? String).flatMap { $0.isEmpty ? nil : $0 }
            return diff
        })
    }

    /// Gemini CLI's edit confirmation: the file before and after, or its patch.
    static func gemini(_ details: [String: Any]) -> [ClaudeFileDiff] {
        guard details["type"] as? String == "edit" else { return [] }
        let path = details["filePath"] as? String ?? details["fileName"] as? String ?? ""
        if let new = details["newContent"] as? String {
            let old = details["originalContent"] as? String
            let modifies = details["isModifying"] as? Bool ?? false
            if old == nil || (old?.isEmpty == true && !modifies) {
                return limited([write(path: path, content: new, exists: false, file: nil)])
            }
            return limited([write(path: path, content: new, exists: true, file: old)])
        }
        guard let text = details["fileDiff"] as? String else { return [] }
        return limited(patch(path: path, text))
    }

    /// OpenCode's edit permission: the patch it would apply, per file when it
    /// patches several.
    static func opencode(_ metadata: [String: Any]) -> [ClaudeFileDiff] {
        if let files = metadata["files"] as? [[String: Any]], !files.isEmpty {
            return limited(files.compactMap { file in
                guard let path = (file["filePath"] as? String) ?? (file["relativePath"] as? String),
                      let text = file["patch"] as? String else { return nil }
                let type = file["type"] as? String
                var diff = patch(path: path, text).first ?? ClaudeFileDiff(path: path)
                diff.path = path
                if type == "add" { diff.change = .add }
                if type == "delete" { diff.change = .delete }
                diff.movedTo = (file["movePath"] as? String).flatMap { $0.isEmpty ? nil : $0 }
                return diff
            })
        }
        guard let text = metadata["diff"] as? String, !text.isEmpty else { return [] }
        let path = (metadata["filepath"] as? String) ?? (metadata["filePath"] as? String) ?? ""
        return limited(patch(path: path, text))
    }

    /// A unified diff read into one diff per file it names. Text with no
    /// hunk at all is a whole file's content, added or removed as `whole` says.
    static func patch(path: String, _ text: String, whole: ClaudeFileDiff.Change? = nil) -> [ClaudeFileDiff] {
        let rows = split(normalized(text))
        guard rows.contains(where: { $0.hasPrefix("@@") }) else {
            guard let whole else { return [] }
            var diff = ClaudeFileDiff(path: path, change: whole)
            diff.lines = hunks(whole == .delete ? match(rows, []) : match([], rows), context: .max, numbered: true)
            count(&diff)
            return [diff]
        }
        var files: [ClaudeFileDiff] = []
        var current: ClaudeFileDiff?
        var named: String?
        var oldLeft = 0, newLeft = 0, oldLine = 0, newLine = 0
        func finish() {
            if var diff = current, !diff.lines.isEmpty {
                count(&diff)
                files.append(diff)
            }
            current = nil
        }
        for row in rows {
            if oldLeft > 0 || newLeft > 0 {
                if row.hasPrefix("\\") { continue }
                let body = String(row.dropFirst())
                switch row.first {
                case "+":
                    current?.lines.append(.init(kind: .added, text: body, number: newLine))
                    newLine += 1; newLeft -= 1
                case "-":
                    current?.lines.append(.init(kind: .removed, text: body, number: oldLine))
                    oldLine += 1; oldLeft -= 1
                default:
                    current?.lines.append(.init(kind: .context, text: body, number: newLine))
                    oldLine += 1; newLine += 1; oldLeft -= 1; newLeft -= 1
                }
                continue
            }
            if row.hasPrefix("@@"), let header = hunkHeader(row) {
                if current == nil { current = ClaudeFileDiff(path: named ?? path) }
                if current?.lines.isEmpty == false { current?.lines.append(.init(kind: .gap, text: "")) }
                (oldLine, oldLeft, newLine, newLeft) = header
            } else if row.hasPrefix("diff --git ") || row.hasPrefix("Index: ") {
                finish()
                named = nil
            } else if row.hasPrefix("+++ ") {
                let name = String(row.dropFirst(4)).split(separator: "\t").first.map(String.init) ?? ""
                if name != "/dev/null" {
                    named = name.hasPrefix("b/") ? String(name.dropFirst(2)) : name
                }
                if current != nil { finish() }
            } else if row.hasPrefix("--- ") {
                if current != nil { finish() }
                let name = String(row.dropFirst(4)).split(separator: "\t").first.map(String.init) ?? ""
                if name != "/dev/null" { named = name.hasPrefix("a/") ? String(name.dropFirst(2)) : name }
            }
        }
        finish()
        // One file: the path the agent gave, which is whole, beats the patch's.
        if files.count == 1, !path.isEmpty { files[0].path = path }
        return files
    }

    /// `@@ -a,b +c,d @@`: where each side starts and how many lines it has.
    static func hunkHeader(_ row: String) -> (Int, Int, Int, Int)? {
        let parts = row.split(separator: " ")
        guard parts.count >= 3, parts[1].hasPrefix("-"), parts[2].hasPrefix("+") else { return nil }
        func range(_ part: Substring) -> (Int, Int)? {
            let numbers = part.dropFirst().split(separator: ",", omittingEmptySubsequences: false)
            guard let start = Int(numbers[0]) else { return nil }
            let length = numbers.count > 1 ? Int(numbers[1]) : 1
            guard let length else { return nil }
            return (start, length)
        }
        guard let old = range(parts[1]), let new = range(parts[2]) else { return nil }
        return (old.0, old.1, new.0, new.1)
    }

    // MARK: Building

    struct Op: Equatable {
        let kind: ClaudeFileDiff.Line.Kind
        let text: String
        /// Indexes from 0 into the old and the new lines.
        let old: Int
        let new: Int
    }

    /// Where `old` sits in `content`, and the lines that change around it.
    static func place(_ old: String, _ new: String, in content: String, all: Bool)
        -> (lines: [ClaudeFileDiff.Line], result: String, count: Int)? {
        let text = content as NSString
        var found: [NSRange] = []
        var from = 0
        while from <= text.length {
            let range = text.range(of: old, options: .literal, range: NSRange(location: from, length: text.length - from))
            guard range.location != NSNotFound, range.length > 0 else { break }
            found.append(range)
            if !all { break }
            from = range.location + range.length
        }
        guard !found.isEmpty else { return nil }
        // Where each line starts, to turn places into whole lines.
        var starts = [0]
        for index in 0..<text.length where text.character(at: index) == 10 { starts.append(index + 1) }
        func line(_ offset: Int) -> Int {
            var low = 0, high = starts.count - 1
            while low < high {
                let mid = (low + high + 1) / 2
                if starts[mid] <= offset { low = mid } else { high = mid - 1 }
            }
            return low
        }
        // Places on the same lines change together.
        var groups: [(first: Int, last: Int, ranges: [NSRange])] = []
        for range in found {
            let first = line(range.location), last = line(range.location + range.length - 1)
            if let previous = groups.last, previous.last >= first {
                groups[groups.count - 1].last = max(previous.last, last)
                groups[groups.count - 1].ranges.append(range)
            } else {
                groups.append((first, last, [range]))
            }
        }
        let oldLines = split(content)
        var ops: [Op] = []
        var oldIndex = 0, newIndex = 0
        var result = ""
        var cursor = 0
        for group in groups {
            while oldIndex < group.first, oldIndex < oldLines.count {
                ops.append(Op(kind: .context, text: oldLines[oldIndex], old: oldIndex, new: newIndex))
                oldIndex += 1; newIndex += 1
            }
            let start = starts[group.first]
            let end = group.last + 1 < starts.count ? starts[group.last + 1] : text.length
            var replaced = ""
            var at = start
            for range in group.ranges {
                replaced += text.substring(with: NSRange(location: at, length: range.location - at)) + new
                at = range.location + range.length
            }
            replaced += text.substring(with: NSRange(location: at, length: end - at))
            result += text.substring(with: NSRange(location: cursor, length: start - cursor)) + replaced
            cursor = end
            let before = split(text.substring(with: NSRange(location: start, length: end - start)))
            for op in match(before, split(replaced)) {
                ops.append(Op(kind: op.kind, text: op.text, old: oldIndex + op.old, new: newIndex + op.new))
            }
            oldIndex += before.count
            newIndex += split(replaced).count
        }
        result += text.substring(from: cursor)
        while oldIndex < oldLines.count {
            ops.append(Op(kind: .context, text: oldLines[oldIndex], old: oldIndex, new: newIndex))
            oldIndex += 1; newIndex += 1
        }
        return (hunks(ops, context: context, numbered: true), result, found.count)
    }

    /// The lines of `old` and `new` matched up: kept, removed and added, in
    /// order. Lines both share at the start and the end are kept without
    /// matching the rest; a middle too large to match is removed, then added.
    static func match(_ old: [String], _ new: [String]) -> [Op] {
        var head = 0
        while head < old.count, head < new.count, old[head] == new[head] { head += 1 }
        var oldEnd = old.count, newEnd = new.count
        while oldEnd > head, newEnd > head, old[oldEnd - 1] == new[newEnd - 1] { oldEnd -= 1; newEnd -= 1 }
        var ops = (0..<head).map { Op(kind: .context, text: new[$0], old: $0, new: $0) }
        let n = oldEnd - head, m = newEnd - head
        if n == 0 || m == 0 || n * m > matchLimit {
            ops += (head..<oldEnd).map { Op(kind: .removed, text: old[$0], old: $0, new: head) }
            ops += (head..<newEnd).map { Op(kind: .added, text: new[$0], old: oldEnd, new: $0) }
        } else {
            // Lines as numbers, which compare far faster than text.
            var ids: [String: Int] = [:]
            func id(_ line: String) -> Int {
                if let known = ids[line] { return known }
                ids[line] = ids.count
                return ids.count - 1
            }
            let a = old[head..<oldEnd].map(id)
            let b = new[head..<newEnd].map(id)
            let width = m + 1
            var table = [Int32](repeating: 0, count: (n + 1) * width)
            for i in stride(from: n - 1, through: 0, by: -1) {
                for j in stride(from: m - 1, through: 0, by: -1) {
                    table[i * width + j] = a[i] == b[j] ? table[(i + 1) * width + j + 1] + 1
                        : max(table[(i + 1) * width + j], table[i * width + j + 1])
                }
            }
            var i = 0, j = 0
            while i < n || j < m {
                if i < n, j < m, a[i] == b[j] {
                    ops.append(Op(kind: .context, text: new[head + j], old: head + i, new: head + j))
                    i += 1; j += 1
                } else if j >= m || (i < n && table[(i + 1) * width + j] >= table[i * width + j + 1]) {
                    ops.append(Op(kind: .removed, text: old[head + i], old: head + i, new: head + j))
                    i += 1
                } else {
                    ops.append(Op(kind: .added, text: new[head + j], old: head + i, new: head + j))
                    j += 1
                }
            }
        }
        ops += (0..<(old.count - oldEnd)).map { Op(kind: .context, text: old[oldEnd + $0], old: oldEnd + $0, new: newEnd + $0) }
        return ops
    }

    /// The changed lines with `context` unchanged ones around each, and a
    /// gap where unchanged lines are left out. Numbered ops are the whole
    /// file, so each gap keeps the lines it leaves out, the file's start
    /// and end included, for the card to show on a click.
    static func hunks(_ ops: [Op], context: Int, numbered: Bool) -> [ClaudeFileDiff.Line] {
        let changes = ops.indices.filter { ops[$0].kind != .context }
        guard !changes.isEmpty else { return [] }
        var keep = [Bool](repeating: context == .max, count: ops.count)
        if context != .max {
            for index in changes {
                for kept in max(0, index - context)...min(ops.count - 1, index + context) { keep[kept] = true }
            }
        }
        func line(_ op: Op) -> ClaudeFileDiff.Line {
            .init(kind: op.kind, text: op.text, number: numbered ? (op.kind == .removed ? op.old : op.new) + 1 : nil)
        }
        var lines: [ClaudeFileDiff.Line] = []
        var skipped: [ClaudeFileDiff.Line] = []
        func gap(_ edge: ClaudeFileDiff.Gap.Edge) {
            defer { skipped = [] }
            guard !skipped.isEmpty else { return }
            guard numbered else {
                if edge == .middle { lines.append(.init(kind: .gap, text: "")) }
                return
            }
            lines.append(.init(kind: .gap, text: "", gap: .init(edge: edge, count: skipped.count, head: skipped)))
        }
        for (index, op) in ops.enumerated() {
            guard keep[index] else { skipped.append(line(op)); continue }
            gap(lines.isEmpty ? .top : .middle)
            lines.append(line(op))
        }
        gap(.bottom)
        return lines
    }

    static func count(_ diff: inout ClaudeFileDiff) {
        diff.added = diff.lines.filter { $0.kind == .added }.count
        diff.removed = diff.lines.filter { $0.kind == .removed }.count
    }

    /// At most `lineLimit` lines across the files, the rest counted, and
    /// each line cut at `textLimit` characters. What the limit leaves over
    /// keeps lines inside the gaps, a click's worth at a time for each side
    /// a gap opens from, nearest the changes first.
    static func limited(_ diffs: [ClaudeFileDiff]) -> [ClaudeFileDiff] {
        var left = lineLimit
        var diffs = diffs.map { diff in
            var diff = diff
            let shown = min(left, diff.lines.count)
            let cutOff = shown < diff.lines.count
            diff.hidden += diff.lines.count - shown
            diff.lines = diff.lines.prefix(shown).map(cut)
            // A gap the limit left last has no change below it any more.
            if cutOff, diff.lines.last?.gap != nil { diff.lines[diff.lines.count - 1].gap = nil }
            left -= shown
            return diff
        }
        // Every side a gap opens from, with the lines it could keep.
        struct Side { let file: Int, line: Int, fromEnd: Bool; var kept = 0 }
        var sides: [Side] = []
        var all: [[Int: [ClaudeFileDiff.Line]]] = diffs.map { _ in [:] }
        for (file, diff) in diffs.enumerated() {
            for (index, line) in diff.lines.enumerated() {
                guard let gap = line.gap else { continue }
                all[file][index] = gap.head
                if gap.edge != .top { sides.append(Side(file: file, line: index, fromEnd: false)) }
                if gap.edge != .bottom { sides.append(Side(file: file, line: index, fromEnd: true)) }
            }
        }
        var grew = true
        while left > 0, grew {
            grew = false
            for index in sides.indices where left > 0 {
                let side = sides[index]
                let count = all[side.file][side.line]?.count ?? 0
                // Both sides of one gap share its lines.
                let other = sides.first { $0.file == side.file && $0.line == side.line && $0.fromEnd != side.fromEnd }
                let more = min(expandStep, left, count - side.kept - (other?.kept ?? 0))
                guard more > 0 else { continue }
                sides[index].kept += more
                left -= more
                grew = true
            }
        }
        for side in sides {
            guard let lines = all[side.file][side.line], var gap = diffs[side.file].lines[side.line].gap else { continue }
            if side.fromEnd {
                gap.tail = lines.suffix(side.kept).map(cut)
            } else {
                gap.head = lines.prefix(side.kept).map(cut)
            }
            diffs[side.file].lines[side.line].gap = gap
        }
        // A side that opens from nowhere keeps nothing.
        for file in diffs.indices {
            for index in diffs[file].lines.indices where diffs[file].lines[index].gap != nil {
                if diffs[file].lines[index].gap?.edge == .top { diffs[file].lines[index].gap?.head = [] }
                if diffs[file].lines[index].gap?.edge == .bottom { diffs[file].lines[index].gap?.tail = [] }
            }
        }
        return diffs
    }

    private static func cut(_ line: ClaudeFileDiff.Line) -> ClaudeFileDiff.Line {
        guard line.text.count > textLimit else { return line }
        return .init(kind: line.kind, text: String(line.text.prefix(textLimit)) + "…", number: line.number, gap: line.gap)
    }

    // MARK: Showing gaps

    /// How far the card has opened one gap: lines shown from its start and
    /// from its end.
    struct Opened: Equatable {
        var down = 0
        var up = 0
    }

    enum Way: Equatable {
        /// The next lines below the change above.
        case down
        /// The next lines above the change below.
        case up
        /// Everything kept.
        case all
    }

    /// One row of a diff as the card shows it.
    enum Row: Equatable {
        /// A line, with a key that stays the same while gaps open around it.
        case line(ClaudeFileDiff.Line, key: String)
        /// A gap still holding lines back. `index` is the gap's place in the
        /// diff's lines; `down`, `up` and `all` are the ways it opens now.
        case gap(index: Int, edge: ClaudeFileDiff.Gap.Edge?, hidden: Int, down: Bool, up: Bool, all: Bool)
    }

    /// The diff's lines with each gap opened as far as `opened` says.
    static func rows(_ lines: [ClaudeFileDiff.Line], opened: [Int: Opened]) -> [Row] {
        var rows: [Row] = []
        for (index, line) in lines.enumerated() {
            guard line.kind == .gap else { rows.append(.line(line, key: "\(index)")); continue }
            guard let gap = line.gap else {
                rows.append(.gap(index: index, edge: nil, hidden: 0, down: false, up: false, all: false))
                continue
            }
            let open = clamped(opened[index] ?? Opened(), gap)
            rows += gap.head.prefix(open.down).enumerated().map { .line($1, key: "\(index)h\($0)") }
            let hidden = gap.count - open.down - open.up
            if hidden > 0 {
                let down = gap.head.count > open.down, up = gap.tail.count > open.up
                // The rest fits one click and is all kept: one way shows it.
                let all = hidden <= expandStep && gap.head.count - open.down + gap.tail.count - open.up == hidden
                // Nothing kept past the file's first or last change: no row,
                // as before the card could open gaps.
                if gap.edge == .middle || !gap.head.isEmpty || !gap.tail.isEmpty {
                    rows.append(.gap(index: index, edge: gap.edge, hidden: hidden, down: down && !all,
                                     up: up && !all, all: all))
                }
            }
            let first = gap.tail.count - open.up
            rows += gap.tail.suffix(open.up).enumerated().map { .line($1, key: "\(index)t\(first + $0)") }
        }
        return rows
    }

    /// `opened` one click further the given way.
    static func open(_ gap: ClaudeFileDiff.Gap, _ opened: Opened, _ way: Way) -> Opened {
        var next = clamped(opened, gap)
        switch way {
        case .down: next.down += expandStep
        case .up: next.up += expandStep
        case .all: next = Opened(down: gap.head.count, up: gap.tail.count)
        }
        return clamped(next, gap)
    }

    /// Never more than a gap keeps, nor lines shown twice.
    private static func clamped(_ opened: Opened, _ gap: ClaudeFileDiff.Gap) -> Opened {
        let down = min(max(0, opened.down), gap.head.count)
        let up = min(max(0, opened.up), gap.tail.count, gap.count - down)
        return Opened(down: down, up: up)
    }

    // MARK: Text

    /// Windows and old Mac line ends read as one.
    static func normalized(_ text: String) -> String {
        // "\r\n" is one Character to Swift, so look at the bytes.
        guard text.utf8.contains(13) else { return text }
        return text.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
    }

    /// The lines of a text; a final line break does not start another.
    static func split(_ text: String) -> [String] {
        guard !text.isEmpty else { return [] }
        var lines = text.components(separatedBy: "\n")
        if lines.last == "" { lines.removeLast() }
        return lines
    }

    static func resolve(_ path: String, _ cwd: String?) -> String {
        let expanded = (path as NSString).expandingTildeInPath
        guard !expanded.hasPrefix("/"), let cwd, !cwd.isEmpty else { return expanded }
        return (cwd as NSString).appendingPathComponent(expanded)
    }

    /// A file's text, when it is small, readable and not binary.
    static func readText(_ path: String) -> String? {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: path),
              attributes[.type] as? FileAttributeType == .typeRegular,
              let size = attributes[.size] as? NSNumber, size.intValue <= fileLimit,
              let data = FileManager.default.contents(atPath: path),
              !data.prefix(8192).contains(0), let text = String(data: data, encoding: .utf8) else { return nil }
        // A byte order mark is not part of the first line.
        return text.hasPrefix("\u{FEFF}") ? String(text.dropFirst()) : text
    }
}
