// SPDX-License-Identifier: GPL-3.0-or-later
import CryptoKit
import Foundation

/// The authoritative text is never replaced with its display preview. Swift strings share storage.
struct PermissionContent: Equatable {
    static let previewLimit = 8000
    static let viewerBytes = 1024 * 1024
    let full: String
    let complete: Bool
    let originalLength: Int?
    // Swift String equality is canonically equivalent Unicode equality. Permission
    // snapshots must also refresh when the original command bytes change.
    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.complete == rhs.complete && lhs.originalLength == rhs.originalLength
            && lhs.full.utf8.elementsEqual(rhs.full.utf8)
    }
    var count: Int { full.unicodeScalars.count }
    var truncated: Bool { count > Self.previewLimit || !complete }
    var remaining: Int? { originalLength.map { max(0, $0 - min(count, Self.previewLimit)) } }
    var requiresInspection: Bool { !complete || full.isEmpty || count > Self.previewLimit || full.unicodeScalars.contains(where: Self.control) }
    var available: Bool { complete && !full.isEmpty && full.utf8.count <= Self.viewerBytes
        && full.unicodeScalars.reduce(0, { $0 + (Self.control($1) ? 16 : String($1).utf8.count) }) <= Self.viewerBytes }
    var preview: String {
        let raw = String(String.UnicodeScalarView(full.unicodeScalars.prefix(Self.previewLimit)))
        let left = max(0, (originalLength ?? count) - min(count, Self.previewLimit))
        return Self.display(raw) + (left > 0 ? "\n… \(left) more characters" : "")
            + (!complete ? "\n… full command unavailable" : "")
    }
    static func control(_ c: Unicode.Scalar) -> Bool {
        let n = c.value
        return n == 0x27e6 || n == 0x27e7 || (n < 32 && n != 10 && n != 9) || (127...159).contains(n) || n == 0x061c
            || (0x200b...0x200f).contains(n) || (0x2028...0x202e).contains(n)
            || (0x2060...0x206f).contains(n) || n == 0xfeff
    }
    /// Visible annotations for ANSI, bidi and other invisible controls; never interpreted.
    /// Full retains every original scalar, including controls and whitespace.
    static func display(_ raw: String) -> String {
        raw.unicodeScalars.map { control($0) ? String(format: "⟦U+%04X⟧", $0.value) : String($0) }.joined()
    }
    func revision(_ permission: ClaudePermission) -> String {
        var hash = SHA256()
        for field in [permission.requestID, permission.tool, String(permission.since.timeIntervalSince1970.bitPattern),
                      full, String(complete), String(originalLength ?? -1)] {
            hash.update(data: Data("\(field.utf8.count):".utf8))
            hash.update(data: Data(field.utf8))
        }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

struct PermissionInspection {
    let revision: String
    let token: String
    let text: String
}

/// Owned by the service/client, accessed only on its queue/under its lock.
/// Neither a disabled button nor a frontend inspection flag is authorization.
final class PermissionInspectionGate {
    private var grants: [String: (session: String, revision: String)] = [:]
    func open(_ session: String, _ permission: ClaudePermission, revision: String) -> PermissionInspection? {
        guard let content = permission.content, content.available, content.revision(permission) == revision else { return nil }
        let token = UUID().uuidString + UUID().uuidString
        grants = grants.filter { $0.value.session != session || $0.value.revision != revision }
        grants[token] = (session, revision)
        return PermissionInspection(revision: revision, token: token, text: PermissionContent.display(content.full))
    }
    func allows(_ session: String, _ permission: ClaudePermission, revision: String?, token: String?) -> Bool {
        guard let content = permission.content else { return false }
        let current = content.revision(permission)
        guard content.complete, !content.full.isEmpty, revision == current else { return false }
        if !content.requiresInspection { return true }
        guard content.available, let token, let grant = grants[token] else { return false }
        return grant.session == session && grant.revision == current
    }
    func invalidate(_ session: String, revision: String) { grants = grants.filter { $0.value.session != session || $0.value.revision != revision } }
    func invalidate(_ session: String) { grants = grants.filter { $0.value.session != session } }
    func close(_ token: String) { grants[token] = nil }
    func retain(_ sessions: [ClaudeSession]) {
        let live = sessions.compactMap { s -> (String, String)? in
            guard let p = s.permission, let content = p.content else { return nil }
            return (s.id, content.revision(p))
        }
        grants = grants.filter { grant in live.contains { $0.0 == grant.value.session && $0.1 == grant.value.revision } }
    }
}
