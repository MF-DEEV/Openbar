// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum ClaudeFileDiffTests {
    static func run(_ suite: TestSuite) {
        suite.run("file diffs: edits") { edits(suite) }
        suite.run("file diffs: writes and notebooks") { writes(suite) }
        suite.run("file diffs: patches") { patches(suite) }
        suite.run("file diffs: limits") { limits(suite) }
        suite.run("file diffs: requests") { requests(suite) }
    }

    typealias Line = ClaudeFileDiff.Line

    static func kinds(_ diff: ClaudeFileDiff?) -> String {
        (diff?.lines ?? []).map { line -> String in
            switch line.kind {
            case .added: return "+" + line.text
            case .removed: return "-" + line.text
            case .context: return " " + line.text
            case .gap: return "~"
            }
        }.joined(separator: "|")
    }

    static func numbers(_ diff: ClaudeFileDiff?) -> [Int?] { (diff?.lines ?? []).filter { $0.kind != .gap }.map(\.number) }

    static func file(_ count: Int) -> String { (1...count).map { "line \($0)" }.joined(separator: "\n") + "\n" }

    static func edit(_ old: String, _ new: String, all: Bool = false, file: String?) -> ClaudeFileDiff? {
        ClaudeFileDiffs.diffs(tool: "Edit", input: ["file_path": "/p/a.swift", "old_string": old, "new_string": new,
                                                    "replace_all": all], read: { _ in file }).first
    }

    // MARK: Edits

    static func edits(_ suite: TestSuite) {
        // Without the file: the edit's own text, unnumbered.
        let bare = edit("a\nb\nc", "a\nB\nc", file: nil)
        suite.expect(kinds(bare) == " a|-b|+B| c", "an edit alone reads old against new: \(kinds(bare))")
        suite.expect(numbers(bare).allSatisfy { $0 == nil } && bare?.added == 1 && bare?.removed == 1 && bare?.change == .update,
                     "without the file there are no line numbers")

        // With the file: three lines around it, numbered as in the file.
        let placed = edit("line 10\n", "ten\n", file: file(20))
        suite.expect(kinds(placed) == " line 7| line 8| line 9|-line 10|+ten| line 11| line 12| line 13",
                     "the edit lands in the file with context: \(kinds(placed))")
        suite.expect(numbers(placed) == [7, 8, 9, 10, 10, 11, 12, 13], "numbered as in the file: \(numbers(placed))")

        // An edit inside a line shows the whole line.
        let inside = edit("ine 2", "INE 2", file: file(3))
        suite.expect(kinds(inside) == " line 1|-line 2|+lINE 2| line 3", "a part of a line shows the line: \(kinds(inside))")

        // Empty strings.
        let created = edit("", "first\nsecond", file: nil)
        suite.expect(created?.change == .add && kinds(created) == "+first|+second" && numbers(created) == [1, 2],
                     "an edit with nothing to replace and no file makes it")
        let removed = edit("gone\n", "", file: nil)
        suite.expect(kinds(removed) == "-gone" && removed?.added == 0 && removed?.removed == 1, "an empty new string removes")
        suite.expect(edit("", "", file: nil)?.lines.isEmpty == true, "nothing to nothing shows no lines")
        let emptyInFile = edit("line 2\n", "", file: file(3))
        suite.expect(kinds(emptyInFile) == " line 1|-line 2| line 3", "removing a whole line in a file: \(kinds(emptyInFile))")

        // replace_all: every place, counted, each with its own context.
        var many = (1...20).map { "row \($0)" }
        many[1] = "foo"; many[14] = "foo"
        let all = edit("foo", "bar", all: true, file: many.joined(separator: "\n"))
        suite.expect(all?.places == 2 && all?.added == 2 && all?.removed == 2, "replace-all counts its places")
        suite.expect(kinds(all).contains("~") && numbers(all).first == 1, "far apart places are two hunks: \(kinds(all))")
        let blind = edit("foo", "bar", all: true, file: nil)
        suite.expect(blind?.everyMatch == true && blind?.places == nil, "without the file it says every match")
        let once = edit("foo", "bar", all: true, file: "foo\n")
        suite.expect(once?.places == nil && once?.everyMatch == false, "one place needs no count")
        let sameLine = edit("o", "0", all: true, file: "foo boo\nzzz\n")
        suite.expect(kinds(sameLine) == "-foo boo|+f00 b00| zzz" && sameLine?.places == 4,
                     "places on one line change together: \(kinds(sameLine))")

        // CRLF in the file, the strings, or both.
        let crlf = edit("two\nthree", "2\n3", file: "one\r\ntwo\r\nthree\r\nfour\r\n")
        suite.expect(kinds(crlf) == " one|-two|-three|+2|+3| four" && numbers(crlf).first == 1,
                     "a CRLF file matches an edit written with LF: \(kinds(crlf))")
        let crlfEdit = edit("two\r\n", "2\r\n", file: "one\ntwo\n")
        suite.expect(kinds(crlfEdit) == " one|-two|+2", "a CRLF edit matches an LF file: \(kinds(crlfEdit))")
        suite.expect(!kinds(crlf).contains("\r"), "no line keeps a carriage return")

        // Not in the file as read: the edit alone.
        let missing = edit("absent", "x", file: file(3))
        suite.expect(kinds(missing) == "-absent|+x" && numbers(missing) == [nil, nil], "an edit not in the file shows alone")

        // MultiEdit: each edit in order, later ones on the file as changed.
        let multi = ClaudeFileDiffs.diffs(tool: "MultiEdit", input: ["file_path": "/p/m.txt", "edits": [
            ["old_string": "line 2", "new_string": "two"],
            ["old_string": "two\nline 3", "new_string": "two\nthree"],
            ["old_string": "line 19", "new_string": "nineteen"],
            ["bad": true],
        ]], read: { _ in file(20) }).first
        let hunks = kinds(multi).components(separatedBy: "~")
        suite.expect(hunks.count == 3, "each edit is its own hunk: \(kinds(multi))")
        suite.expect(hunks.count == 3 && hunks[1].contains("-line 3|+three") && hunks[1].contains(" two"),
                     "a later edit sees the earlier one's text: \(hunks.count > 1 ? hunks[1] : "")")
        suite.expect(multi?.added == 3 && multi?.removed == 3, "counts sum over the edits")
        suite.expect(ClaudeFileDiffs.diffs(tool: "MultiEdit", input: ["file_path": "/p", "edits": []], read: { _ in nil }).isEmpty,
                     "no edits, no diff")
        suite.expect(ClaudeFileDiffs.diffs(tool: "Edit", input: ["file_path": "/p"], read: { _ in nil }).isEmpty,
                     "an edit missing its strings shows no diff")
        suite.expect(ClaudeFileDiffs.diffs(tool: "Bash", input: ["command": "ls"], read: { _ in nil }).isEmpty,
                     "other tools have none")
    }

    // MARK: Writes and notebooks

    static func writes(_ suite: TestSuite) {
        let fresh = ClaudeFileDiffs.write(path: "/p/new.txt", content: "a\nb\n", exists: false, file: nil)
        suite.expect(fresh.change == .add && kinds(fresh) == "+a|+b" && numbers(fresh) == [1, 2], "a new file is all added")
        let over = ClaudeFileDiffs.write(path: "/p/old.txt", content: file(10).replacingOccurrences(of: "line 5\n", with: "five\n"),
                                         exists: true, file: file(10))
        suite.expect(over.change == .update && kinds(over) == " line 2| line 3| line 4|-line 5|+five| line 6| line 7| line 8",
                     "a written file reads against the one on disk: \(kinds(over))")
        let unread = ClaudeFileDiffs.write(path: "/p/locked", content: "x", exists: true, file: nil)
        suite.expect(unread.change == .replace && kinds(unread) == "+x", "an unreadable file is written over, its text unknown")
        let same = ClaudeFileDiffs.write(path: "/p/same", content: "a\n", exists: true, file: "a\r\n")
        suite.expect(same.lines.isEmpty && same.added == 0 && same.removed == 0, "only line ends differ: no change")
        let empty = ClaudeFileDiffs.write(path: "/p/empty", content: "", exists: false, file: nil)
        suite.expect(empty.change == .add && empty.lines.isEmpty, "an empty new file has no lines")
        let cleared = ClaudeFileDiffs.write(path: "/p/c", content: "", exists: true, file: "a\nb")
        suite.expect(kinds(cleared) == "-a|-b", "emptying a file removes every line")

        // The real file system: a missing file is new; a directory can't be read.
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("vs-diff-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let onDisk = folder.appendingPathComponent("f.txt")
        try? "keep\nold\n".write(to: onDisk, atomically: true, encoding: .utf8)
        let disk = ClaudeFileDiffs.diffs(tool: "Write", input: ["file_path": "f.txt", "content": "keep\nnew\n"],
                                         cwd: folder.path).first
        suite.expect(kinds(disk) == " keep|-old|+new", "a relative path reads from the session's folder: \(kinds(disk))")
        let absent = ClaudeFileDiffs.diffs(tool: "Write", input: ["file_path": folder.appendingPathComponent("no.txt").path,
                                                                  "content": "x"]).first
        suite.expect(absent?.change == .add, "a missing file is new")
        let binary = folder.appendingPathComponent("b.bin")
        try? Data([0x61, 0, 0x62]).write(to: binary)
        suite.expect(ClaudeFileDiffs.readText(binary.path) == nil && ClaudeFileDiffs.readText(folder.path) == nil,
                     "binary files and folders are not read")
        let big = folder.appendingPathComponent("big.txt")
        try? Data(repeating: 0x61, count: ClaudeFileDiffs.fileLimit + 1).write(to: big)
        suite.expect(ClaudeFileDiffs.readText(big.path) == nil, "files past the limit are not read")
        let tooBig = ClaudeFileDiffs.diffs(tool: "Write", input: ["file_path": big.path, "content": "a"]).first
        suite.expect(tooBig?.change == .replace, "a file too big to read is written over")

        // Notebooks.
        let notebook = ##"{"cells":[{"id":"intro","cell_type":"markdown","source":["# Title\n","text"]},{"cell_type":"code","source":"print(1)\nprint(2)"}]}"##
        func cell(_ input: [String: Any]) -> ClaudeFileDiff? {
            var all = input
            all["notebook_path"] = "/p/n.ipynb"
            return ClaudeFileDiffs.diffs(tool: "NotebookEdit", input: all, read: { _ in notebook }).first
        }
        let replaced = cell(["cell_id": "intro", "new_source": "# Title\nmore"])
        suite.expect(kinds(replaced) == " # Title|-text|+more" && replaced?.cell == "intro" && replaced?.change == .update,
                     "a replaced cell reads against its source: \(kinds(replaced))")
        let byIndex = cell(["cell_id": "cell-1", "new_source": "print(3)\nprint(2)"])
        suite.expect(kinds(byIndex) == "-print(1)|+print(3)| print(2)", "cells without ids are found by number: \(kinds(byIndex))")
        let inserted = cell(["cell_id": "intro", "new_source": "x = 1", "edit_mode": "insert"])
        suite.expect(inserted?.change == .add && inserted?.cell != nil && kinds(inserted) == "+x = 1", "an inserted cell is new")
        let deleted = cell(["cell_id": "intro", "edit_mode": "delete"])
        suite.expect(deleted?.change == .delete && kinds(deleted) == "-# Title|-text", "a deleted cell is all removed")
        let unknown = cell(["cell_id": "nope", "new_source": "y"])
        suite.expect(kinds(unknown) == "+y" && numbers(unknown) == [nil], "an unknown cell shows its new source alone")
    }

    // MARK: Patches

    static func patches(_ suite: TestSuite) {
        let update = ClaudeFileDiffs.codex([["path": "/w/a.rs", "kind": ["type": "update", "move_path": NSNull()],
                                             "diff": "@@ -3,3 +3,3 @@\n keep\n-old\n+new\n tail\n"]]).first
        suite.expect(kinds(update) == " keep|-old|+new| tail" && numbers(update) == [3, 4, 4, 5] && update?.path == "/w/a.rs",
                     "a Codex patch reads with its line numbers: \(kinds(update)) \(numbers(update))")
        let add = ClaudeFileDiffs.codex([["path": "/w/n.txt", "kind": ["type": "add"], "diff": "one\ntwo\n"]]).first
        suite.expect(add?.change == .add && kinds(add) == "+one|+two", "a Codex new file is its content")
        let delete = ClaudeFileDiffs.codex([["path": "/w/d.txt", "kind": ["type": "delete"], "diff": "bye\n"]]).first
        suite.expect(delete?.change == .delete && kinds(delete) == "-bye", "a deleted file is its content, removed")
        let moved = ClaudeFileDiffs.codex([["path": "/w/a", "kind": ["type": "update", "move_path": "/w/b"], "diff": ""]]).first
        suite.expect(moved?.movedTo == "/w/b" && moved?.lines.isEmpty == true, "a move names where to")
        let two = ClaudeFileDiffs.codex([
            ["path": "/w/1", "kind": ["type": "add"], "diff": "x"], ["path": "/w/2", "kind": ["type": "add"], "diff": "y"],
        ])
        suite.expect(two.map(\.path) == ["/w/1", "/w/2"], "each file of a change shows")

        // Several hunks with a gap, and no newline markers skipped.
        let hunks = ClaudeFileDiffs.patch(path: "/f", "--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n-a\n+A\n b\n@@ -10 +10 @@\n-j\n\\ No newline at end of file\n+J\n")
        suite.expect(kinds(hunks.first) == "-a|+A| b|~|-j|+J" && numbers(hunks.first) == [1, 1, 2, 10, 10],
                     "hunks keep their own numbers: \(kinds(hunks.first))")
        // A removed line that looks like a header stays a line.
        let tricky = ClaudeFileDiffs.patch(path: "/f", "@@ -1,2 +1,1 @@\n--- not a header\n keep\n")
        suite.expect(kinds(tricky.first) == "--- not a header| keep", "header-like lines inside a hunk are lines")

        // OpenCode: one file's patch with its headers, or a patch per file.
        let opencode = ClaudeFileDiffs.opencode(["filepath": "/w/o.ts", "diff":
            "Index: /w/o.ts\n===================================================================\n--- /w/o.ts\n+++ /w/o.ts\n@@ -1,1 +1,1 @@\n-a\n+b\n"])
        suite.expect(opencode.count == 1 && opencode.first?.path == "/w/o.ts" && kinds(opencode.first) == "-a|+b",
                     "OpenCode's edit patch reads: \(opencode.map(\.path))")
        let files = ClaudeFileDiffs.opencode(["filepath": "a, b", "diff": "x", "files": [
            ["filePath": "/w/a", "type": "update", "patch": "@@ -1 +1 @@\n-1\n+2\n"],
            ["filePath": "/w/b", "type": "add", "patch": "@@ -0,0 +1 @@\n+new\n"],
        ]])
        suite.expect(files.map(\.path) == ["/w/a", "/w/b"] && files.last?.change == .add, "a patch of several files shows each")
        suite.expect(ClaudeFileDiffs.opencode(["command": "ls"]).isEmpty, "other OpenCode permissions have none")

        // Gemini CLI: before and after.
        let gemini = ClaudeFileDiffs.gemini(["type": "edit", "filePath": "/w/g.py", "fileName": "g.py",
                                             "originalContent": "a\nb\n", "newContent": "a\nc\n", "isModifying": false])
        suite.expect(kinds(gemini.first) == " a|-b|+c" && gemini.first?.path == "/w/g.py", "Gemini's edit reads before and after")
        let geminiNew = ClaudeFileDiffs.gemini(["type": "edit", "filePath": "/w/n.py", "originalContent": NSNull(), "newContent": "z"])
        suite.expect(geminiNew.first?.change == .add, "Gemini's new file is new")
        let geminiPatch = ClaudeFileDiffs.gemini(["type": "edit", "filePath": "/w/p.py", "fileDiff": "@@ -1 +1 @@\n-q\n+r\n"])
        suite.expect(kinds(geminiPatch.first) == "-q|+r", "without contents Gemini's patch reads")
        suite.expect(ClaudeFileDiffs.gemini(["type": "exec", "command": "ls"]).isEmpty, "Gemini commands have none")
    }

    // MARK: Limits

    static func limits(_ suite: TestSuite) {
        let long = (1...5000).map { "l\($0)" }.joined(separator: "\n")
        let huge = ClaudeFileDiffs.diffs(tool: "Write", input: ["file_path": "/nope/\(UUID().uuidString)", "content": long],
                                         read: { _ in nil }).first
        suite.expect(huge?.added == 5000 && huge?.lines.count == ClaudeFileDiffs.lineLimit
                         && huge?.hidden == 5000 - ClaudeFileDiffs.lineLimit, "past the limit lines are counted, not kept")
        let wide = ClaudeFileDiffs.diffs(tool: "Edit", input: ["file_path": "/p", "old_string": "a",
                                                               "new_string": String(repeating: "w", count: 5000)],
                                         read: { _ in nil }).first
        suite.expect(wide?.lines.last?.text.count == ClaudeFileDiffs.textLimit + 1 && wide?.lines.last?.text.hasSuffix("…") == true,
                     "a very long line is cut")
        // Many files share one budget.
        let changes = (1...3).map { index -> [String: Any] in
            ["path": "/w/\(index)", "kind": ["type": "add"], "diff": (1...300).map(String.init).joined(separator: "\n")]
        }
        let shared = ClaudeFileDiffs.codex(changes)
        suite.expect(shared.map(\.lines.count) == [300, 100, 0] && shared.map(\.hidden) == [0, 200, 300],
                     "files share the line budget: \(shared.map(\.lines.count))")
        // Two big different files fall back to removed-then-added rather than matching line by line.
        let a = (1...3000).map { "a\($0)" }, b = (1...3000).map { "b\($0)" }
        let start = Date()
        let ops = ClaudeFileDiffs.match(a, b)
        suite.expect(ops.filter { $0.kind == .removed }.count == 3000 && ops.filter { $0.kind == .added }.count == 3000
                         && Date().timeIntervalSince(start) < 2, "a middle too large to match is replaced whole")
        // A big file with a small edit stays quick: shared ends are not matched.
        let bigFile = (1...50_000).map { "row \($0)" }.joined(separator: "\n")
        let quick = Date()
        let one = edit("row 25000\n", "changed\n", file: bigFile)
        suite.expect(one?.added == 1 && numbers(one).first == 24997 && Date().timeIntervalSince(quick) < 2,
                     "an edit in a big file is placed quickly")
    }

    // MARK: Requests

    static func requests(_ suite: TestSuite) {
        let json = #"{"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"Edit","cwd":"/tmp","tool_input":{"file_path":"/nonexistent/\#(UUID().uuidString).swift","old_string":"let a = 1","new_string":"let a = 2"}}"#
        let message = ClaudeHookMessage.decode(Data(json.utf8), requestID: "r", now: Date(), policy: ClaudeContentPolicy(previews: false))
        guard case .permissionRequested(let ask)? = message?.fact.event else { return suite.expect(false, "an edit request decodes") }
        suite.expect(kinds(ask.diffs.first) == "-let a = 1|+let a = 2", "the request carries the diff, previews or not")
        suite.expect(!ask.fitsOnALine, "a file change is never a one-line quick decision")
        let bash = #"{"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"ls"}}"#
        if case .permissionRequested(let other)? = ClaudeHookMessage.decode(Data(bash.utf8), requestID: "r", now: Date(),
                                                                            policy: ClaudeContentPolicy())?.fact.event {
            suite.expect(other.diffs.isEmpty && other.fitsOnALine, "a command has no diff")
        }

        // Codex: the approval names its item; the item carried the patch.
        let state = CodexSessionTests.stateWithThread()
        state.notify("item/started", params: ["threadId": "t1", "item": [
            "type": "fileChange", "id": "fc1", "status": "inProgress",
            "changes": [["path": "/w/x.swift", "kind": ["type": "update"], "diff": "@@ -1 +1 @@\n-a\n+b\n"]],
        ]], now: CodexSessionTests.at(1))
        _ = state.request(.number(4), method: "item/fileChange/requestApproval",
                          params: ["threadId": "t1", "turnId": "u", "itemId": "fc1"], now: CodexSessionTests.at(2))
        let card = state.snapshot(now: CodexSessionTests.at(2), previews: false).sessions.first?.permission
        suite.expect(kinds(card?.diffs.first) == "-a|+b" && card?.diffs.first?.path == "/w/x.swift",
                     "a Codex approval shows its item's patch")
        suite.expect(card?.summary == "x.swift" && card?.detail == "/w/x.swift", "and names the file: \(card?.detail ?? "")")
        // A patch that changes while the request waits is shown as it is now.
        state.notify("item/fileChange/patchUpdated", params: ["threadId": "t1", "turnId": "u", "itemId": "fc1",
                                                             "changes": [["path": "/w/x.swift", "kind": ["type": "update"],
                                                                          "diff": "@@ -1 +1 @@\n-a\n+c\n"]]],
                     now: CodexSessionTests.at(3))
        let updated = state.snapshot(now: CodexSessionTests.at(3), previews: false).sessions.first?.permission
        suite.expect(kinds(updated?.diffs.first) == "-a|+c", "an updated patch replaces the shown one")
        // An item that ends takes its patch with it.
        state.notify("item/completed", params: ["threadId": "t1", "item": ["type": "fileChange", "id": "fc1", "status": "completed",
                                                                           "changes": []]], now: CodexSessionTests.at(4))
        suite.expect(state.threads["t1"]?.fileChanges.isEmpty == true, "the patch is dropped when the item ends")
        // No item seen: the reason, as before.
        _ = state.takePending(thread: "t1", requestID: "codex:4")
        _ = state.request(.number(5), method: "item/fileChange/requestApproval",
                          params: ["threadId": "t1", "itemId": "unknown", "reason": "write outside"], now: CodexSessionTests.at(5))
        let bare = state.snapshot(now: CodexSessionTests.at(5), previews: false).sessions.first?.permission
        suite.expect(bare?.diffs.isEmpty == true, "an approval without its item has no diff")
    }
}
