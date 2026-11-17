// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// A session's own git worktree, so sessions started on one repository
/// don't edit each other's files. Claude Code makes its own with
/// `--worktree <name>` and `claude rm` takes it away; for every other agent
/// the island makes one in `<repo>/.openbar/worktrees/<name>`, on a branch
/// `openbar/<name>` from `HEAD`, and removes only those.
enum SessionWorktrees {
    /// Where the island keeps the worktrees it makes, inside the repository
    /// so the agents' folder trust carries over, and left out of `git status`
    /// through the repository's own `info/exclude`.
    static let ownFolder = ".openbar/worktrees"
    static let branchPrefix = "openbar/"
    static let claudeBranchPrefix = "worktree-"

    /// Folders agents keep worktrees in, inside the repository they belong to.
    static let markers = ["/.claude/worktrees/", "/.openbar/worktrees/", "/.gemini/worktrees/"]

    /// What a folder is to git.
    enum RepoState: Equatable {
        case notRepo
        /// Its checkout matches `HEAD`.
        case clean(root: String)
        /// It has changes a new worktree, made from `HEAD`, won't have.
        case dirty(root: String)

        var root: String? {
            switch self {
            case .notRepo: return nil
            case .clean(let root), .dirty(let root): return root
            }
        }
    }

    // MARK: Names

    /// A name from the first prompt: `Fix the login bug!` as `fix-the-login-bug`.
    static func slug(_ text: String, limit: Int = 40) -> String {
        var words: [String] = []
        var word = ""
        for scalar in asciiFolded(text).unicodeScalars {
            if scalar.isASCII, CharacterSet.alphanumerics.contains(scalar) {
                word.unicodeScalars.append(scalar)
            } else if !word.isEmpty {
                words.append(word)
                word = ""
            }
        }
        if !word.isEmpty { words.append(word) }
        var slug = ""
        for word in words {
            let next = slug.isEmpty ? word : "\(slug)-\(word)"
            if next.count > limit {
                if slug.isEmpty { slug = String(word.prefix(limit)) }
                break
            }
            slug = next
        }
        return slug.lowercased()
    }

    /// Latin letters as plain ASCII: `ż` as `z`, `ł` as `l`, `ß` as `ss`.
    /// Windows folds the same table, so a prompt names the same worktree there.
    static func asciiFolded(_ text: String) -> String {
        var out = ""
        for character in text.decomposedStringWithCanonicalMapping.unicodeScalars {
            if character.isASCII { out.unicodeScalars.append(character); continue }
            if let plain = asciiLetters[character] { out += plain }
            else if character.properties.generalCategory == .nonspacingMark { continue }
            else { out += " " }
        }
        return out
    }

    /// Letters that don't decompose into an ASCII one and a mark.
    static let asciiLetters: [Unicode.Scalar: String] = [
        "ł": "l", "Ł": "L", "ø": "o", "Ø": "O", "ß": "ss", "æ": "ae", "Æ": "AE", "œ": "oe", "Œ": "OE",
        "đ": "d", "Đ": "D", "ð": "d", "Ð": "D", "þ": "th", "Þ": "TH", "ı": "i",
    ]

    /// `base`, or `base-2`, `base-3`… when that is taken.
    static func unique(_ base: String, taken: Set<String>) -> String {
        guard taken.contains(base) else { return base }
        var number = 2
        while taken.contains("\(base)-\(number)") { number += 1 }
        return "\(base)-\(number)"
    }

    /// The worktree's name: the one typed, else one from the prompt, else
    /// `session`, made unique.
    static func name(custom: String?, prompt: String, taken: Set<String>) -> String {
        let typed = slug(custom ?? "")
        let base = typed.isEmpty ? slug(prompt) : typed
        return unique(base.isEmpty ? "session" : base, taken: taken)
    }

    // MARK: Paths

    /// The repository a worktree kept inside it belongs to; the path itself
    /// for any other folder.
    static func repository(of path: String) -> String {
        for marker in markers {
            if let range = path.range(of: marker) { return String(path[..<range.lowerBound]) }
        }
        return path
    }

    /// The name of the worktree a session works in, when it works in one an
    /// agent or the island made.
    static func worktreeName(_ cwd: String) -> String? {
        for marker in markers {
            guard let range = cwd.range(of: marker) else { continue }
            let name = cwd[range.upperBound...].split(separator: "/").first.map(String.init)
            if let name, !name.isEmpty { return name }
        }
        return nil
    }

    /// The worktree's folder, where the session's own may be deeper inside it.
    static func worktreeFolder(_ cwd: String) -> String? {
        for marker in markers {
            guard let range = cwd.range(of: marker),
                  let name = cwd[range.upperBound...].split(separator: "/").first else { continue }
            return String(cwd[..<range.upperBound]) + name
        }
        return nil
    }

    /// The folder in the worktree that matches the one picked, which may be
    /// deeper in the repository than its root.
    static func inside(_ worktree: String, for folder: String, root: String) -> String {
        let trimmedRoot = root.hasSuffix("/") ? String(root.dropLast()) : root
        for base in [trimmedRoot, "/private" + trimmedRoot, trimmedRoot.replacingOccurrences(of: "/private/", with: "/")] {
            guard folder.hasPrefix(base + "/") else { continue }
            let rest = folder.dropFirst(base.count + 1)
            return rest.isEmpty ? worktree : worktree + "/" + rest
        }
        return worktree
    }

    /// Whether the island made it, so the island may remove it.
    static func isOwn(_ cwd: String) -> Bool { cwd.contains("/\(ownFolder)/") }

    // MARK: git

    /// `git`, without the stub in `/usr/bin` that asks to install the
    /// command line tools when they are missing.
    static func gitExecutable(path: String?, isExecutable: (String) -> Bool = FileManager.default.isExecutableFile(atPath:),
                              toolsInstalled: () -> Bool = SessionWorktrees.developerToolsInstalled) -> String? {
        let onPath = (path ?? "").split(separator: ":").map { "\($0)/git" }.filter { !$0.hasPrefix("/usr/bin/") }
        let usual = ["/opt/homebrew/bin/git", "/usr/local/bin/git"]
        if let found = (onPath + usual).first(where: isExecutable) { return found }
        return toolsInstalled() && isExecutable("/usr/bin/git") ? "/usr/bin/git" : nil
    }

    static func developerToolsInstalled() -> Bool {
        let manager = FileManager.default
        return manager.fileExists(atPath: "/Library/Developer/CommandLineTools/usr/bin/git")
            || manager.fileExists(atPath: "/Applications/Xcode.app/Contents/Developer/usr/bin/git")
    }

    static func git(_ arguments: [String], in folder: String, timeout: TimeInterval = 15) -> ClaudeCLI.Output? {
        let shell = ClaudeCLI.shell()
        guard let git = gitExecutable(path: shell.environment["PATH"]) else { return nil }
        var environment = shell.environment
        // Never stop to ask for anything: nobody sees the question.
        environment["GIT_TERMINAL_PROMPT"] = "0"
        environment["GIT_OPTIONAL_LOCKS"] = "0"
        return ClaudeCLI.run(git, ["-C", folder] + arguments, environment: environment, timeout: timeout)
    }

    private static func text(_ output: ClaudeCLI.Output?) -> String? {
        guard let output, output.status == 0 else { return nil }
        return String(decoding: output.out, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Whether a folder is in a git checkout, and whether that checkout has
    /// changes to tracked files. A bare repository has no checkout.
    static func state(of folder: String) -> RepoState {
        guard let lines = text(git(["rev-parse", "--is-inside-work-tree", "--show-toplevel"], in: folder))?
            .split(separator: "\n").map(String.init),
              lines.first == "true", let root = lines.dropFirst().first, !root.isEmpty else { return .notRepo }
        let changes = text(git(["status", "--porcelain", "--untracked-files=no"], in: root)) ?? ""
        return changes.isEmpty ? .clean(root: root) : .dirty(root: root)
    }

    /// Names in use: worktree folders the agents and the island keep, and
    /// the branches they would make.
    static func takenNames(root: String) -> Set<String> {
        existingNames(root: root).union(Reservations.shared.names(root: root))
    }

    private static func existingNames(root: String) -> Set<String> {
        var taken = Set<String>()
        for folder in [".claude/worktrees", ownFolder, ".gemini/worktrees"] {
            let url = URL(fileURLWithPath: root).appending(path: folder)
            taken.formUnion((try? FileManager.default.contentsOfDirectory(atPath: url.path)) ?? [])
        }
        let branches = text(git(["for-each-ref", "--format=%(refname:short)", "refs/heads"], in: root)) ?? ""
        for branch in branches.split(separator: "\n").map(String.init) {
            if branch.hasPrefix(branchPrefix) { taken.insert(String(branch.dropFirst(branchPrefix.count))) }
            if branch.hasPrefix(claudeBranchPrefix) { taken.insert(String(branch.dropFirst(claudeBranchPrefix.count))) }
        }
        return taken
    }

    /// Picks the name and holds it a while, so two sessions started at once
    /// don't both take it before either has made its folder.
    static func reserveName(custom: String?, prompt: String, root: String) -> String {
        let existing = existingNames(root: root)
        return Reservations.shared.reserve(root: root) { reserved in
            name(custom: custom, prompt: prompt, taken: existing.union(reserved))
        }
    }

    /// Makes `<root>/.openbar/worktrees/<name>` on a new branch from `HEAD`.
    /// Should another session have taken the name meanwhile, the next free
    /// one is used. The worktree's path, or what git said went wrong.
    static func create(root: String, name: String) -> Result<String, CreateFailure> {
        excludeOwnFolder(root: root)
        var name = name
        var said = ""
        for _ in 0..<5 {
            let path = URL(fileURLWithPath: root).appending(path: "\(ownFolder)/\(name)").path
            let output = git(["worktree", "add", "-b", branchPrefix + name, path, "HEAD"], in: root, timeout: 300)
            if output?.status == 0 { return .success(path) }
            said = output.map { lastLine($0.error) } ?? ""
            guard FileManager.default.fileExists(atPath: path) || said.contains("already exists") else { break }
            name = unique(name, taken: takenNames(root: root).union([name]))
        }
        return .failure(CreateFailure(message: said))
    }

    struct CreateFailure: Error, Equatable { let message: String }

    /// What git said last, which names what went wrong.
    static func lastLine(_ text: String) -> String {
        text.split(separator: "\n").last.map { $0.trimmingCharacters(in: .whitespaces) } ?? ""
    }

    /// Keeps the island's folder out of `git status` without touching any
    /// file the repository shares.
    static func excludeOwnFolder(root: String) {
        guard let common = text(git(["rev-parse", "--path-format=absolute", "--git-common-dir"], in: root)), !common.isEmpty
        else { return }
        let info = URL(fileURLWithPath: common).appending(path: "info")
        let file = info.appending(path: "exclude")
        let current = (try? String(contentsOf: file, encoding: .utf8)) ?? ""
        let line = "/.openbar/"
        guard !current.split(separator: "\n").contains(where: { $0.trimmingCharacters(in: .whitespaces) == line }) else { return }
        try? FileManager.default.createDirectory(at: info, withIntermediateDirectories: true)
        let separator = current.isEmpty || current.hasSuffix("\n") ? "" : "\n"
        try? (current + separator + line + "\n").write(to: file, atomically: true, encoding: .utf8)
    }

    enum RemoveResult: Equatable {
        case removed
        /// It has changes that would be lost; nothing was removed.
        case unsaved
        case failed(String)
    }

    /// Removes a worktree the island made. One with changes stays unless
    /// `force` says to lose them. Its branch is kept, so commits on it never
    /// go with it.
    static func remove(_ path: String, force: Bool) -> RemoveResult {
        guard isOwn(path), let folder = worktreeFolder(path) else { return .failed("") }
        let root = repository(of: folder)
        guard FileManager.default.fileExists(atPath: folder) else {
            _ = git(["worktree", "prune"], in: root)
            return .removed
        }
        if !force, let changes = text(git(["status", "--porcelain"], in: folder)), !changes.isEmpty { return .unsaved }
        let output = git(["worktree", "remove"] + (force ? ["--force"] : []) + [folder], in: root, timeout: 60)
        _ = git(["worktree", "prune"], in: root)
        if output?.status == 0 { return .removed }
        return .failed(output.map { lastLine($0.error) } ?? "")
    }

    /// What `claude rm` said when it kept a session's worktree: the changes
    /// would be lost, and the value its `--discard-unpushed` takes to lose
    /// them anyway, when it gives one.
    struct ClaudeKept: Equatable {
        let discard: String?
    }

    static func claudeKept(_ said: String) -> ClaudeKept? {
        guard said.contains("its worktree is still at") else { return nil }
        guard let range = said.range(of: "--discard-unpushed ") else { return ClaudeKept(discard: nil) }
        let value = said[range.upperBound...].prefix { !$0.isWhitespace && $0 != "'" && $0 != "\"" }
        return ClaudeKept(discard: value.isEmpty ? nil : String(value))
    }

    /// `claude rm` on a background session. A worktree with changes found
    /// nowhere else keeps the session unless `anyway` says to lose them:
    /// uncommitted changes are dropped first, as `claude rm` has no flag for
    /// them, then unpushed commits go with the `--discard-unpushed` it names.
    static func removeClaudeJob(_ job: String, configHome: String?, cwd: String, anyway: Bool,
                                run: (([String]) -> ClaudeCLI.Output?)? = nil,
                                discard: (String) -> Bool = SessionWorktrees.discardChanges) -> WorktreeRemoval {
        let remove = { (extra: [String]) in
            run?(["rm", job] + extra) ?? (run == nil ? ClaudeCLI.claude(["rm", job] + extra, configHome: configHome) : nil)
        }
        // It says what it kept on standard output.
        let said = { (output: ClaudeCLI.Output) in
            [String(decoding: output.out, as: UTF8.self), output.error].filter { !$0.isEmpty }.joined(separator: "\n")
        }
        let keptBy = { (output: ClaudeCLI.Output?) in output.flatMap { $0.status == 0 ? nil : claudeKept(said($0)) } }
        var output = remove([])
        var kept = keptBy(output)
        if kept != nil, anyway {
            if kept?.discard == nil, let folder = worktreeFolder(cwd), discard(folder) {
                output = remove([])
                kept = keptBy(output)
            }
            if let discard = kept?.discard {
                output = remove(["--discard-unpushed", discard])
                kept = keptBy(output)
            }
        }
        if kept != nil { return .unsaved }
        guard let output else { return .failed("") }
        return output.status == 0 ? .done : .failed(lastLine(said(output)))
    }

    /// Drops a worktree's uncommitted changes, which `claude rm` has no flag
    /// for, once the person has said to lose them.
    static func discardChanges(_ folder: String) -> Bool {
        git(["reset", "--hard", "-q"], in: folder)?.status == 0 && git(["clean", "-fdq"], in: folder)?.status == 0
    }

    /// The branch a worktree is on; nil on a detached `HEAD`.
    static func branch(_ folder: String) -> String? {
        text(git(["symbolic-ref", "--quiet", "--short", "HEAD"], in: folder)).flatMap { $0.isEmpty ? nil : $0 }
    }

    final class Reservations: @unchecked Sendable {
        static let shared = Reservations()
        private let lock = NSLock()
        private var held: [String: (root: String, until: Date)] = [:]
        private static let span: TimeInterval = 120

        func names(root: String) -> Set<String> {
            lock.lock()
            defer { lock.unlock() }
            let now = Date()
            held = held.filter { $0.value.until > now }
            return Set(held.filter { $0.value.root == root }.map { String($0.key.dropFirst(root.count + 1)) })
        }

        func reserve(root: String, pick: (Set<String>) -> String) -> String {
            lock.lock()
            defer { lock.unlock() }
            let now = Date()
            held = held.filter { $0.value.until > now }
            let reserved = Set(held.filter { $0.value.root == root }.map { String($0.key.dropFirst(root.count + 1)) })
            let name = pick(reserved)
            held["\(root)/\(name)"] = (root, now.addingTimeInterval(Self.span))
            return name
        }
    }
}

/// How removing a session or its worktree went.
enum WorktreeRemoval: Equatable {
    case done
    /// Its worktree has changes found nowhere else; nothing was removed.
    case unsaved
    case failed(String)
}

extension ClaudeSession {
    /// The worktree it works in, when an agent or the island made it one.
    var worktree: String? { SessionWorktrees.worktreeName(cwd) }
}
