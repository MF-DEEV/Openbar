// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Why an ask stays out of Allow all, or `.safe` when it may join it (#269).
/// Only the queue reads this: every ask can still be allowed on its own.
/// A list of what looks dangerous, not a sandbox: a tool the island does not
/// know, or an ask it could not read, never counts as safe.
enum PermissionRisk: String, Equatable, CaseIterable {
    case safe
    /// Not looked at yet; never allowed in bulk.
    case unassessed
    /// It matches a `deny` or `ask` rule in the Claude Code settings.
    case rule
    case deletes
    /// It writes, reads or moves outside the session's project folder.
    case outside
    case network
    /// Privileges, processes, permissions or system settings.
    case system
    /// It runs code it builds or fetches: `eval`, a pipe into a shell,
    /// `python -c`.
    case runsCode
    /// A tool the island does not know, an MCP server's included.
    case unfamiliar

    var isSafe: Bool { self == .safe }

    /// The most telling reason first, when an ask has several.
    static let order: [PermissionRisk] = [.rule, .deletes, .outside, .network, .system, .runsCode, .unfamiliar, .unassessed]

    static func worst(_ risks: [PermissionRisk]) -> PermissionRisk {
        order.first { risks.contains($0) } ?? .safe
    }

    // MARK: Asks

    /// What an ask would do, judged from what the card shows: the tool, its
    /// whole command or path, and the files a change touches.
    static func assess(_ permission: ClaudePermission, cwd: String, rules: PermissionRules = .none,
                       home: String = NSHomeDirectory(), resolveLinks: Bool = true) -> PermissionRisk {
        let full = permission.content?.full ?? permission.detail
        let root = Paths(cwd: cwd, home: home, resolveLinks: resolveLinks)
        let kind = Kind(tool: permission.tool)
        var paths: [String] = []
        switch kind {
        case .write, .delete:
            paths = permission.diffs.flatMap { [$0.path] + [$0.movedTo].compactMap { $0 } }
            if paths.isEmpty { paths = Self.paths(in: full) }
        case .read: paths = Self.paths(in: full)
        default: break
        }
        let script = kind == .command ? ShellScript.parse(full) : []
        if rules.matches(tool: permission.tool, kind: kind, text: full, paths: paths, segments: script, root: root) {
            return .rule
        }
        switch kind {
        case .command: return command(script, root: root)
        case .read: return paths.allSatisfy(root.inside) ? .safe : .outside
        case .write:
            if paths.isEmpty { return .unfamiliar }
            if permission.diffs.contains(where: { $0.change == .delete }) { return .deletes }
            return paths.allSatisfy(root.inside) ? .safe : .outside
        case .delete: return .deletes
        case .network: return .network
        case .folder: return .outside
        case .system: return .system
        case .benign: return .safe
        case .unknown: return .unfamiliar
        }
    }

    /// What kind of thing a tool does, by the name the island gives it
    /// (Claude Code's own, or another agent's mapped onto it).
    enum Kind: Equatable {
        case command, read, write, delete, network, folder, system, benign, unknown

        init(tool: String) {
            switch tool {
            case "Bash", "PowerShell", "Command": self = .command
            case "Read", "Glob", "Grep", "LS", "NotebookRead": self = .read
            case "Edit", "Write", "MultiEdit", "NotebookEdit": self = .write
            case "Delete": self = .delete
            case "WebFetch", "WebSearch": self = .network
            // OpenCode asks before it works in a folder outside the project.
            case "Folder": self = .folder
            // Codex asking for more sandbox access.
            case "Permissions": self = .system
            case "Task", "Agent", "TodoWrite", "TodoRead", "BashOutput", "Skill",
                 "TaskCreate", "TaskUpdate", "TaskGet", "TaskList", "TaskOutput": self = .benign
            default: self = .unknown
            }
        }
    }

    /// The paths a read or a change names: its `path:` style arguments, or
    /// the whole text when it is one bare path.
    static func paths(in text: String) -> [String] {
        let lines = text.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }
        let keyed = lines.compactMap { line -> String? in
            for key in ["file_path: ", "notebook_path: ", "path: ", "filePath: ", "filepath: "] where line.hasPrefix(key) {
                return String(line.dropFirst(key.count))
            }
            return nil
        }
        // A glob names a folder only when it starts from one.
        let patterns = lines.filter { $0.hasPrefix("pattern: /") || $0.hasPrefix("pattern: ~") }.map { String($0.dropFirst(9)) }
        if !keyed.isEmpty || !patterns.isEmpty { return keyed + patterns }
        // Codex lists a change's reason above its paths; OpenCode its patterns.
        return lines.filter { !$0.isEmpty && !$0.contains(": ") && !$0.contains(" ") || $0.hasPrefix("/") || $0.hasPrefix("~") }
    }

    // MARK: Commands

    static let deleting: Set<String> = ["rm", "rmdir", "unlink", "shred", "srm", "trash", "del", "erase", "rd",
                                        "remove-item", "ri", "truncate", "clear-content"]
    static let networking: Set<String> = ["curl", "wget", "ssh", "scp", "sftp", "rsync", "nc", "ncat", "netcat", "telnet",
                                          "ftp", "tftp", "http", "https", "xh", "ping", "gh", "npx", "bunx", "pnpx",
                                          "invoke-webrequest", "iwr", "invoke-restmethod", "irm", "start-bitstransfer",
                                          "winget", "choco", "scoop", "aria2c", "mosh"]
    static let privileged: Set<String> = ["sudo", "su", "doas", "dd", "fdisk", "diskutil", "chmod", "chown", "chgrp",
                                          "chflags", "xattr", "kill", "killall", "pkill", "taskkill", "stop-process",
                                          "shutdown", "reboot", "halt", "launchctl", "systemctl", "service", "crontab",
                                          "defaults", "reg", "set-executionpolicy", "csrutil", "spctl", "security",
                                          "mount", "umount", "passwd", "useradd", "usermod", "sysctl", "iptables",
                                          "ufw", "netsh", "sc", "icacls", "takeown", "setx", "pmset", "nvram", "tccutil",
                                          "runas", "start-process"]
    static let evaluating: Set<String> = ["eval", "source", ".", "iex", "invoke-expression", "osascript"]
    static let interpreters: Set<String> = ["sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "pwsh",
                                            "powershell", "cmd", "python", "python3", "python2", "node", "deno", "bun",
                                            "perl", "ruby", "php", "lua", "osascript"]
    /// Commands that write the paths they are given.
    static let writing: Set<String> = ["tee", "mkdir", "touch", "new-item", "ni", "set-content", "out-file", "add-content",
                                       "chmod", "ln", "patch"]
    /// Commands whose last path is where they write.
    static let copying: Set<String> = ["cp", "mv", "install", "ditto", "copy-item", "move-item", "copy", "move", "cpi", "mi"]
    /// Package managers that download when they add or update packages.
    static let packages: [String: Set<String>] = [
        "npm": ["install", "i", "add", "ci", "update", "up", "upgrade", "publish", "exec", "x", "dlx", "login", "create", "init"],
        "pnpm": ["install", "i", "add", "update", "up", "publish", "dlx", "create"],
        "yarn": ["install", "add", "upgrade", "up", "publish", "dlx", "create"],
        "bun": ["install", "i", "add", "update", "publish", "x", "create"],
        "pip": ["install", "download"], "pip3": ["install", "download"], "uv": ["add", "pip", "sync", "tool"],
        "brew": ["install", "upgrade", "update", "tap", "reinstall"], "apt": ["install", "update", "upgrade"],
        "apt-get": ["install", "update", "upgrade"], "dnf": ["install", "update", "upgrade"], "yum": ["install", "update"],
        "pacman": ["-s", "-syu", "-sy"], "apk": ["add", "update"], "cargo": ["install", "publish", "add", "update", "login"],
        "gem": ["install", "update", "push"], "go": ["get", "install", "mod"], "docker": ["pull", "push", "login", "run", "build"],
        "podman": ["pull", "push", "login", "run"], "composer": ["install", "require", "update"],
        "dotnet": ["add", "restore", "nuget"], "pod": ["install", "update"], "swift": ["package"],
    ]

    static func command(_ script: [ShellScript.Segment], root: Paths) -> PermissionRisk {
        guard !script.isEmpty else { return .safe }
        return worst(script.map { segment(of: $0, root: root, depth: 0) })
    }

    private static func segment(of segment: ShellScript.Segment, root: Paths, depth: Int) -> PermissionRisk {
        var risks: [PermissionRisk] = []
        for target in segment.redirects where target != "/dev/null" && target.lowercased() != "nul" && !target.hasPrefix("&") {
            if !root.inside(target) { risks.append(.outside) }
        }
        let words = ShellScript.command(segment.words)
        guard let first = words.first else { return worst(risks) }
        let name = ShellScript.name(first)
        let args = Array(words.dropFirst())
        let plain = args.filter { !$0.hasPrefix("-") }
        if segment.piped, interpreters.contains(name) { risks.append(.runsCode) }
        if evaluating.contains(name) { risks.append(.runsCode) }
        if deleting.contains(name) { risks.append(.deletes) }
        if networking.contains(name) { risks.append(.network) }
        if privileged.contains(name) || name.hasPrefix("mkfs") { risks.append(.system) }
        if writing.contains(name), !plain.allSatisfy(root.inside) { risks.append(.outside) }
        if copying.contains(name), let last = plain.last, !root.inside(last) { risks.append(.outside) }
        if name == "sed" || name == "perl", args.contains(where: { $0.hasPrefix("-i") }), !plain.dropFirst().allSatisfy(root.inside) {
            risks.append(.outside)
        }
        if ["cd", "pushd", "set-location", "sl", "chdir"].contains(name), let target = plain.first, !root.inside(target) {
            risks.append(.outside)
        }
        let findActions: Set<String> = ["-delete", "-exec", "-execdir", "-ok"]
        if name == "find", args.contains(where: findActions.contains) {
            risks.append(args.contains("-delete") ? .deletes : .runsCode)
        }
        let verbs: Set<String> = packages[name] ?? []
        if verbs.contains(plain.first?.lowercased() ?? "") || verbs.contains(args.first?.lowercased() ?? "") {
            risks.append(.network)
        }
        if name == "npm" || name == "pnpm" || name == "yarn", args.contains("-g") || args.contains("--global") { risks.append(.system) }
        if name == "git" { risks.append(git(args, root: root)) }
        // A shell given its script inline is judged by that script; any
        // other interpreter's inline code can do anything.
        if interpreters.contains(name), let flag = args.firstIndex(where: { ["-c", "-e", "/c", "/k", "-command", "--eval", "-ec", "-lc", "-ic"].contains($0.lowercased()) }) {
            let inline = args.indices.contains(flag + 1) ? args[flag + 1] : ""
            if ["sh", "bash", "zsh", "dash", "ksh", "fish", "cmd", "pwsh", "powershell"].contains(name), depth < 3 {
                risks.append(worst(ShellScript.parse(inline).map { Self.segment(of: $0, root: root, depth: depth + 1) }))
            } else {
                risks.append(.runsCode)
            }
        }
        return worst(risks)
    }

    private static func git(_ args: [String], root: Paths) -> PermissionRisk {
        var args = args[...]
        var risks: [PermissionRisk] = []
        // Options before the verb; -C moves where it works.
        while let first = args.first, first.hasPrefix("-") {
            args = args.dropFirst()
            if first == "-C" || first == "-c" || first == "--git-dir" || first == "--work-tree", let value = args.first {
                if first != "-c", !root.inside(value) { risks.append(.outside) }
                args = args.dropFirst()
            }
        }
        guard let verb = args.first else { return worst(risks) }
        let rest = Array(args.dropFirst())
        switch verb {
        case "push", "pull", "fetch", "clone", "ls-remote", "remote" where rest.first == "update", "submodule" where rest.contains("update"),
             "send-email", "lfs":
            risks.append(.network)
        case "clean", "rm", "restore": risks.append(.deletes)
        case "reset" where rest.contains("--hard") || rest.contains("--merge") || rest.contains("--keep"): risks.append(.deletes)
        case "checkout" where rest.contains("--") || rest.contains(".") || rest.contains("-f") || rest.contains("--force"):
            risks.append(.deletes)
        case "stash" where rest.first == "drop" || rest.first == "clear": risks.append(.deletes)
        case "branch" where rest.contains("-D") || rest.contains("-d") || rest.contains("--delete"): risks.append(.deletes)
        case "tag" where rest.contains("-d") || rest.contains("--delete"): risks.append(.deletes)
        case "worktree" where rest.first == "remove" || rest.first == "prune": risks.append(.deletes)
        case "config" where rest.contains("--global") || rest.contains("--system"): risks.append(.system)
        case "filter-branch", "filter-repo", "gc", "prune", "reflog" where rest.first == "expire" || rest.first == "delete":
            risks.append(.deletes)
        default: break
        }
        return worst(risks)
    }

    // MARK: Paths

    /// Whether a path stays inside the project folder, after `~`, `..` and
    /// symbolic links.
    struct Paths {
        let cwd: String
        let home: String
        var resolveLinks = true

        func absolute(_ path: String) -> String {
            var path = path
            if path.count >= 2, path.first == path.last, path.first == "\"" || path.first == "'" {
                path = String(path.dropFirst().dropLast())
            }
            if path == "~" { path = home } else if path.hasPrefix("~/") { path = home + path.dropFirst(1) }
            if path.hasPrefix("$HOME/") { path = home + path.dropFirst(5) }
            // A Windows drive or share path is never inside a Mac project.
            if !path.hasPrefix("/") { path = cwd + "/" + path }
            return (path as NSString).standardizingPath
        }

        func resolved(_ path: String) -> String {
            let absolute = absolute(path)
            guard resolveLinks else { return absolute }
            // Links in the part that exists; the rest is yet to be written.
            var existing = absolute
            var tail: [String] = []
            while !FileManager.default.fileExists(atPath: existing), existing != "/" {
                tail.insert((existing as NSString).lastPathComponent, at: 0)
                existing = (existing as NSString).deletingLastPathComponent
            }
            let real = (existing as NSString).resolvingSymlinksInPath
            return ([real] + tail).joined(separator: "/").replacingOccurrences(of: "//", with: "/")
        }

        func inside(_ path: String) -> Bool {
            // No project folder to stay in: nothing counts as inside it.
            if cwd.isEmpty || absolute(cwd) == "/" || absolute(cwd) == absolute(home) { return false }
            if path.isEmpty { return true }
            // A glob is judged by the folder it starts from.
            let fixed = path.firstIndex { "*?[{".contains($0) }.map { String(path[..<$0]) } ?? path
            if fixed.isEmpty { return true }
            if fixed.range(of: #"^[A-Za-z]:[\\/]|^\\\\"#, options: .regularExpression) != nil { return false }
            if fixed.contains("$") || fixed.contains("`") { return false }
            let root = resolved(cwd)
            let target = resolved(fixed)
            return target == root || target.hasPrefix(root.hasSuffix("/") ? root : root + "/")
        }
    }
}

// MARK: Shell text

/// A rough reading of a shell command, enough to find what each part runs.
/// Quotes and escapes are honoured; anything it cannot follow splits into
/// more parts, which only ever makes an ask look riskier.
enum ShellScript {
    struct Segment: Equatable {
        var words: [String] = []
        /// Files its output goes to (`>`, `>>`, `2>`, `&>`).
        var redirects: [String] = []
        /// Its input comes from the part before it through `|`.
        var piped = false
        /// The text of the part, for rules that match a command's start.
        var text = ""
    }

    static func parse(_ command: String) -> [Segment] {
        var segments: [Segment] = []
        var current = Segment()
        var word = ""
        var inWord = false
        var quote: Character? = nil
        var redirectNext = false
        var inputNext = false
        // Open substitutions and subshells: what closes each, and the quote
        // to go back into.
        var subs: [(close: Character, resume: Character?)] = []
        var text = ""
        func endWord() {
            guard inWord else { return }
            inWord = false
            // An empty quote, or what is left of one around a substitution.
            if word.isEmpty, !redirectNext { inputNext = false; return }
            if redirectNext { current.redirects.append(word); redirectNext = false }
            else if inputNext { inputNext = false }
            else { current.words.append(word) }
            word = ""
            inWord = false
        }
        func endSegment(piped: Bool) {
            endWord()
            current.text = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if !current.words.isEmpty || !current.redirects.isEmpty { segments.append(current) }
            current = Segment()
            current.piped = piped
            text = ""
        }
        let chars = Array(command)
        var i = 0
        while i < chars.count {
            let c = chars[i]
            let next: Character? = i + 1 < chars.count ? chars[i + 1] : nil
            if let open = quote {
                if c == open { quote = nil; text.append(c); i += 1; continue }
                if open == "\"" {
                    if c == "\\", let next { word.append(next); text.append(c); text.append(next); i += 2; continue }
                    // Inside double quotes a substitution still runs; the
                    // quote goes on once it closes.
                    if c == "`" || (c == "$" && next == "(") {
                        endSegment(piped: false)
                        subs.append((close: c == "`" ? "`" : ")", resume: "\""))
                        quote = nil
                        i += c == "$" ? 2 : 1
                        continue
                    }
                }
                word.append(c); text.append(c); i += 1
                continue
            }
            switch c {
            case "'", "\"":
                quote = c; inWord = true; text.append(c)
            case "\\":
                if let next, next != "\n" { word.append(next); inWord = true; text.append(c); text.append(next) }
                i += 1
            case " ", "\t":
                endWord(); text.append(c)
            case "\n", "\r", ";":
                endSegment(piped: false)
            case "(":
                endSegment(piped: false); subs.append((close: ")", resume: nil))
            case ")" where subs.last?.close == ")", "`" where subs.last?.close == "`":
                endSegment(piped: false)
                if let resume = subs.popLast()?.resume { quote = resume; inWord = true }
            case "`":
                endSegment(piped: false); subs.append((close: "`", resume: nil))
            case ")":
                endSegment(piped: false)
            case "$" where next == "(":
                endSegment(piped: false); subs.append((close: ")", resume: nil)); i += 1
            case "<" where next == "(":
                endSegment(piped: false); subs.append((close: ")", resume: nil)); i += 1
            case "<":
                // What a command reads from is not what it runs or writes.
                endWord(); inputNext = true; text.append(c)
                while i + 1 < chars.count, chars[i + 1] == "<" { i += 1 }
            case "&":
                if next == "&" {
                    endSegment(piped: false); i += 1
                } else if next == ">" {
                    // `&>` and `&>>` send both streams to a file.
                    endWord(); redirectNext = true; i += 1
                    if i + 1 < chars.count, chars[i + 1] == ">" { i += 1 }
                } else {
                    endSegment(piped: false)
                }
            case "|":
                if next == "|" { endSegment(piped: false); i += 1 } else { endSegment(piped: true); if next == "&" { i += 1 } }
            case ">":
                // `2>` and `1>` name the stream, not a word.
                if inWord, word == "1" || word == "2" { word = ""; inWord = false }
                endWord()
                if next == "(" { endSegment(piped: true); subs.append((close: ")", resume: nil)); i += 1; break }
                redirectNext = true
                if next == ">" || next == "|" { i += 1 }
                // `>&2` and `2>&1` join a stream, not a file.
                if i + 1 < chars.count, chars[i + 1] == "&" {
                    redirectNext = false; i += 1
                    while i + 1 < chars.count, chars[i + 1].isNumber || chars[i + 1] == "-" { i += 1 }
                }
                text.append(c)
            default:
                word.append(c); inWord = true; text.append(c)
            }
            i += 1
        }
        endSegment(piped: false)
        return segments
    }

    /// The words that name the command, past assignments and the wrappers
    /// that only run what follows them.
    static func command(_ words: [String]) -> [String] {
        var words = words[...]
        while let first = words.first {
            if first.range(of: #"^[A-Za-z_][A-Za-z0-9_]*="#, options: .regularExpression) != nil { words = words.dropFirst(); continue }
            switch name(first) {
            case "{", "}", "!", "if", "then", "else", "elif", "do", "while", "until", "fi", "done", "&":
                words = words.dropFirst()
            case "command", "builtin", "exec", "nohup", "time", "caffeinate", "stdbuf", "unbuffer", "noglob":
                words = words.dropFirst()
                while let flag = words.first, flag.hasPrefix("-") { words = words.dropFirst() }
            case "env":
                words = words.dropFirst()
                while let flag = words.first, flag.hasPrefix("-") || flag.contains("=") {
                    words = words.dropFirst()
                    if flag == "-u" || flag == "-C" || flag == "-S" { words = words.dropFirst() }
                }
            case "nice", "ionice", "timeout", "gtimeout":
                words = words.dropFirst()
                while let flag = words.first, flag.hasPrefix("-") {
                    words = words.dropFirst()
                    if flag == "-n" || flag == "-s" || flag == "-k" || flag == "-c" { words = words.dropFirst() }
                }
                if name(first).hasSuffix("timeout"), let duration = words.first, duration.first?.isNumber == true { words = words.dropFirst() }
            case "xargs", "parallel", "watch":
                words = words.dropFirst()
                while let flag = words.first, flag.hasPrefix("-") {
                    words = words.dropFirst()
                    if ["-I", "-n", "-P", "-L", "-d", "-E", "-s", "-a", "-j"].contains(flag) { words = words.dropFirst() }
                }
            default:
                return Array(words)
            }
        }
        return Array(words)
    }

    /// A command's name without its folder or `.exe`, in lower case.
    static func name(_ word: String) -> String {
        var name = word.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? word
        if name.lowercased().hasSuffix(".exe") { name = String(name.dropLast(4)) }
        return name.lowercased()
    }
}

// MARK: Settings rules

/// The `deny` and `ask` rules of the Claude Code settings a session reads:
/// an ask they name always waits for the person (#269).
struct PermissionRules: Equatable {
    var rules: [String] = []

    static let none = PermissionRules()

    /// Managed settings, the account's own, then the project's.
    static func read(configHome: String?, cwd: String?, managed: [String] = ClaudePlanModes.managedSettings,
                     home: String = NSHomeDirectory()) -> PermissionRules {
        var files = managed
        files.append(((configHome?.isEmpty == false ? configHome! : home + "/.claude") as NSString).appendingPathComponent("settings.json"))
        if let cwd, !cwd.isEmpty {
            files += [cwd + "/.claude/settings.json", cwd + "/.claude/settings.local.json"]
        }
        var found = PermissionRules()
        for path in files {
            guard let data = FileManager.default.contents(atPath: path),
                  let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
                  let permissions = json["permissions"] as? [String: Any] else { continue }
            for key in ["deny", "ask"] {
                found.rules += (permissions[key] as? [Any] ?? []).compactMap { ($0 as? String).flatMap { $0.isEmpty ? nil : $0 } }
            }
        }
        return found
    }

    func matches(tool: String, kind: PermissionRisk.Kind, text: String, paths: [String], segments: [ShellScript.Segment],
                 root: PermissionRisk.Paths) -> Bool {
        rules.contains { Self.matches($0, tool: tool, kind: kind, text: text, paths: paths, segments: segments, root: root) }
    }

    /// A rule as Claude Code writes them: `Tool`, `Tool(spec)`,
    /// `Bash(prefix:*)`, `Read(./.env)`, `WebFetch(domain:host)`,
    /// `mcp__server`. It errs on matching, which only keeps an ask out of
    /// Allow all.
    static func matches(_ rule: String, tool: String, kind: PermissionRisk.Kind, text: String, paths: [String],
                        segments: [ShellScript.Segment], root: PermissionRisk.Paths) -> Bool {
        let trimmed = rule.trimmingCharacters(in: .whitespaces)
        let name: String
        var spec: String?
        if let open = trimmed.firstIndex(of: "("), trimmed.hasSuffix(")") {
            name = String(trimmed[..<open])
            spec = String(trimmed[trimmed.index(after: open)..<trimmed.index(before: trimmed.endIndex)])
        } else {
            name = trimmed
        }
        guard covers(name, tool: tool, kind: kind) else { return false }
        guard let spec, !spec.isEmpty, spec != "*" else { return true }
        switch kind {
        case .command:
            let lines = segments.map { ShellScript.command($0.words).joined(separator: " ") } + segments.map(\.text) + [text]
            if spec.hasSuffix(":*") {
                let prefix = String(spec.dropLast(2))
                return lines.contains { $0 == prefix || $0.hasPrefix(prefix + " ") || $0.hasPrefix(prefix) }
            }
            return lines.contains { glob(spec, $0, slash: false) }
        case .read, .write, .delete:
            let pattern: String
            if spec.hasPrefix("//") { pattern = String(spec.dropFirst()) }
            else if spec.hasPrefix("~/") { pattern = root.home + spec.dropFirst(1) }
            else if spec.hasPrefix("/") { pattern = root.cwd + spec }
            else if spec.hasPrefix("./") { pattern = root.cwd + spec.dropFirst(1) }
            else { pattern = spec }
            return paths.contains { path in
                let absolute = root.absolute(path)
                if spec.hasPrefix("/"), glob(spec, absolute, slash: true) { return true }
                if !pattern.hasPrefix("/") {
                    // gitignore style: a bare name matches at any depth.
                    return glob("**/" + pattern, absolute, slash: true) || glob(pattern, (absolute as NSString).lastPathComponent, slash: true)
                }
                return glob(pattern, absolute, slash: true) || glob(pattern + "/**", absolute, slash: true)
            }
        case .network:
            if spec.hasPrefix("domain:") {
                let domain = spec.dropFirst(7).lowercased()
                let host = (URL(string: text)?.host ?? text).lowercased()
                return host == domain || host.hasSuffix("." + domain) || glob(String(domain), host, slash: false)
            }
            return glob(spec, text, slash: false)
        default:
            return glob(spec, text, slash: false)
        }
    }

    private static func covers(_ name: String, tool: String, kind: PermissionRisk.Kind) -> Bool {
        if name == tool { return true }
        if name.hasPrefix("mcp__") {
            let server = name.hasSuffix("__*") ? String(name.dropLast(3)) : name
            return tool == server || tool.hasPrefix(server + "__") || ("mcp__" + tool).hasPrefix(server)
        }
        switch name {
        case "Bash", "PowerShell": return kind == .command
        // Edit rules cover every tool that changes files; Read rules every
        // tool that reads them.
        case "Edit", "Write", "MultiEdit", "NotebookEdit": return kind == .write || kind == .delete
        case "Read", "Glob", "Grep", "LS": return kind == .read
        default: return false
        }
    }

    /// `*` within a part, `**` across parts when `slash`; anything else literal.
    static func glob(_ pattern: String, _ text: String, slash: Bool) -> Bool {
        var regex = "^"
        let chars = Array(pattern)
        var i = 0
        while i < chars.count {
            let c = chars[i]
            if c == "*" {
                if i + 1 < chars.count, chars[i + 1] == "*" {
                    regex += ".*"
                    i += 1
                    if slash, i + 1 < chars.count, chars[i + 1] == "/" { regex += "/?"; i += 1 }
                } else {
                    regex += slash ? "[^/]*" : ".*"
                }
            } else if c == "?" {
                regex += slash ? "[^/]" : "."
            } else {
                regex += NSRegularExpression.escapedPattern(for: String(c))
            }
            i += 1
        }
        regex += "$"
        return text.range(of: regex, options: [.regularExpression, .caseInsensitive]) != nil
    }
}

// MARK: Publishing

/// Gives every ask the island publishes its risk, judged once per ask, with
/// each project's settings rules read again at most every few seconds.
/// Used on one queue.
final class PermissionRiskCache {
    static let rulesLifetime: TimeInterval = 5
    private var judged: [String: PermissionRisk] = [:]
    private var rules: [String: (rules: PermissionRules, read: Date)] = [:]
    private let read: (_ configHome: String?, _ cwd: String) -> PermissionRules

    init(read: @escaping (_ configHome: String?, _ cwd: String) -> PermissionRules = { PermissionRules.read(configHome: $0, cwd: $1) }) {
        self.read = read
    }

    func assess(_ sessions: [ClaudeSession], now: Date) -> [ClaudeSession] {
        var seen: Set<String> = []
        let result = sessions.map { session -> ClaudeSession in
            guard session.permission != nil else { return session }
            var session = session
            // A host's files are not on this Mac: its rules cannot be read,
            // nor its links followed.
            let local = session.host.remoteAlias == nil
            let found = local ? rules(session.account, session.cwd, now: now) : .none
            func judge(_ ask: ClaudePermission) -> ClaudePermission {
                var ask = ask
                let key = session.id + "\u{1}" + ask.requestID + "\u{1}" + (ask.content.map { $0.revision(ask) } ?? ask.detail)
                seen.insert(key)
                if let known = judged[key] {
                    ask.risk = known
                } else {
                    ask.risk = PermissionRisk.assess(ask, cwd: session.cwd, rules: found, resolveLinks: local)
                    judged[key] = ask.risk
                }
                return ask
            }
            session.permission = session.permission.map(judge)
            session.laterAsks = session.laterAsks.map(judge)
            return session
        }
        judged = judged.filter { seen.contains($0.key) }
        rules = rules.filter { now.timeIntervalSince($0.value.read) < Self.rulesLifetime * 4 }
        return result
    }

    private func rules(_ configHome: String?, _ cwd: String, now: Date) -> PermissionRules {
        let key = (configHome ?? "") + "\u{1}" + cwd
        if let known = rules[key], now.timeIntervalSince(known.read) < Self.rulesLifetime { return known.rules }
        let found = read(configHome, cwd)
        // A changed rule judges every ask again.
        if let before = rules[key], before.rules != found { judged = [:] }
        rules[key] = (found, now)
        return found
    }
}
