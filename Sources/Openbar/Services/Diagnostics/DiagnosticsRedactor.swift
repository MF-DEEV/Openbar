// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// Masks what a diagnostics file must not carry: the home folder (shown as
/// `~`), email addresses, and keys and tokens. Prompts, replies, commands and
/// file contents are never collected in the first place; this pass catches
/// what an error message or a path might still hold.
///
/// The Windows and Linux app masks the same way (`diagnostics/redact.rs`);
/// both are checked against `Tests/Fixtures/diagnostics-redaction.json`.
enum DiagnosticsRedactor {
    static let secret = "<secret>"
    static let email = "<email>"

    static func redact(_ text: String, home: String = FileManager.default.homeDirectoryForCurrentUser.path) -> String {
        guard !text.isEmpty else { return text }
        var text = text
        text = maskHome(text, home: home)
        text = replace(genericHomes, in: text) { groups in
            let name = groups[groups.count - 1] ?? ""
            return keptFolders.contains(name.lowercased()) ? nil : "~"
        }
        text = replace(prefixedSecrets, in: text) { _ in secret }
        text = replace(bearer, in: text) { groups in "\(groups[1] ?? "") \(secret)" }
        text = replace(keyValue, in: text) { groups in "\(groups[1] ?? "")\(groups[2] ?? "")\(secret)" }
        text = replace(longRun, in: text) { groups in
            let run = groups[0] ?? ""
            let mixed = run.contains(where: \.isUppercase) && run.contains(where: \.isLowercase)
                && run.contains(where: \.isNumber)
            return mixed ? secret : nil
        }
        text = replace(emailPattern, in: text) { _ in email }
        return text
    }

    // MARK: Patterns (kept in step with redact.rs)

    /// Folders under Users that belong to nobody in particular.
    private static let keptFolders: Set<String> = ["shared", "public", "default", "all users", "default user"]

    /// `/Users/<name>`, `/home/<name>` and `C:\Users\<name>` (with `/` or a
    /// JSON-escaped `\\` too). The name is the last group.
    private static let genericHomes = pattern(#"(?i)(?:/Users/|/home/|\b[a-z]:(?:\\\\|\\|/)users(?:\\\\|\\|/))([^\\/\s"'<>:]+)"#)
    private static let prefixedSecrets = pattern(
        #"sk-ant-[A-Za-z0-9_-]{8,}|sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{16,}|github_pat_[A-Za-z0-9_]{16,}|xox[abposr]-[A-Za-z0-9-]{8,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"#)
    private static let bearer = pattern(#"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=-]{8,}"#)
    private static let keyValue = pattern(
        #"(?i)([A-Za-z0-9_-]*(?:api[_-]?key|access[_-]?key|secret|token|password|passwd|credential)s?)(["']?\s*[:=]\s*["']?)[^\s"',;&<]+"#)
    private static let longRun = pattern(#"[A-Za-z0-9_+]{40,}"#)
    private static let emailPattern = pattern(#"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"#)

    private static func pattern(_ source: String) -> NSRegularExpression {
        // Every pattern here is a literal; a typo is a crash in the tests, not at a person's desk.
        try! NSRegularExpression(pattern: source)
    }

    /// The exact home folder, for one outside /Users (a test's, or a network home).
    private static func maskHome(_ text: String, home: String) -> String {
        let trimmed = home.hasSuffix("/") ? String(home.dropLast()) : home
        guard trimmed.count > 1 else { return text }
        let escaped = NSRegularExpression.escapedPattern(for: trimmed)
        guard let regex = try? NSRegularExpression(pattern: escaped + #"(?![A-Za-z0-9_.-])"#) else { return text }
        return replace(regex, in: text) { _ in "~" }
    }

    /// Replaces each match with what `transform` makes of its groups (index 0
    /// is the whole match); nil keeps the match as it was.
    private static func replace(_ regex: NSRegularExpression, in text: String,
                                transform: ([String?]) -> String?) -> String {
        let whole = NSRange(text.startIndex..., in: text)
        let matches = regex.matches(in: text, range: whole)
        guard !matches.isEmpty else { return text }
        let source = text as NSString
        let result = NSMutableString(string: text)
        for match in matches.reversed() {
            let groups: [String?] = (0..<match.numberOfRanges).map { index in
                let range = match.range(at: index)
                return range.location == NSNotFound ? nil : source.substring(with: range)
            }
            if let replacement = transform(groups) {
                result.replaceCharacters(in: match.range, with: replacement)
            }
        }
        return result as String
    }
}
