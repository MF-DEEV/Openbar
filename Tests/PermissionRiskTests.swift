// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation

/// The Allow-all risk check (#269), against the case list the Windows and
/// Linux build tests too.
enum PermissionRiskTests {
    static let casesPath = "windows/src-tauri/src/sessions/risk_cases.json"

    static func permission(tool: String, text: String, diffs: [ClaudeFileDiff] = []) -> ClaudePermission {
        let content = PermissionContent(full: text, complete: true, originalLength: text.unicodeScalars.count)
        return ClaudePermission(requestID: UUID().uuidString, tool: tool, summary: nil, detail: content.preview,
                                content: content, offersAlways: true, since: Date(), diffs: diffs)
    }

    static func run(_ suite: TestSuite) {
        suite.run("permission risk: shared cases") {
            guard let data = FileManager.default.contents(atPath: casesPath),
                  let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
                  let cases = json["cases"] as? [[String: Any]], let cwd = json["cwd"] as? String,
                  let home = json["home"] as? String else {
                suite.expect(false, "risk cases readable at \(casesPath)")
                return
            }
            let rules = PermissionRules(rules: json["rules"] as? [String] ?? [])
            for item in cases {
                let tool = item["tool"] as? String ?? ""
                let text = item["text"] as? String ?? ""
                let diffs = (item["diffs"] as? [[String: Any]] ?? []).map {
                    ClaudeFileDiff(path: $0["path"] as? String ?? "",
                                   change: ClaudeFileDiff.Change(rawValue: $0["change"] as? String ?? "") ?? .update)
                }
                let got = PermissionRisk.assess(permission(tool: tool, text: text, diffs: diffs), cwd: cwd, rules: rules,
                                                home: home, resolveLinks: false)
                suite.expect(got.rawValue == item["expect"] as? String,
                             "\(tool) `\(text)` is \(item["expect"] ?? "?"), got \(got.rawValue)")
            }
        }

        suite.run("permission risk: no project folder, nothing inside") {
            let write = permission(tool: "Write", text: "/w/a.txt")
            suite.expect(PermissionRisk.assess(write, cwd: "", home: "/w", resolveLinks: false) == .outside, "empty cwd")
            suite.expect(PermissionRisk.assess(write, cwd: "/w", home: "/w", resolveLinks: false) == .outside, "cwd is home")
            suite.expect(PermissionRisk.assess(write, cwd: "/", home: "/w", resolveLinks: false) == .outside, "cwd is root")
        }

        suite.run("permission risk: links out of the project count as outside") {
            let base = FileManager.default.temporaryDirectory.appendingPathComponent("risk-\(UUID().uuidString)").path
            defer { try? FileManager.default.removeItem(atPath: base) }
            try? FileManager.default.createDirectory(atPath: base + "/proj", withIntermediateDirectories: true)
            try? FileManager.default.createDirectory(atPath: base + "/other", withIntermediateDirectories: true)
            try? FileManager.default.createSymbolicLink(atPath: base + "/proj/link", withDestinationPath: base + "/other")
            let inside = permission(tool: "Write", text: base + "/proj/new/a.txt")
            let linked = permission(tool: "Write", text: base + "/proj/link/a.txt")
            suite.expect(PermissionRisk.assess(inside, cwd: base + "/proj", home: "/nowhere") == .safe, "new file inside")
            suite.expect(PermissionRisk.assess(linked, cwd: base + "/proj", home: "/nowhere") == .outside, "link leads out")
        }

        suite.run("permission risk: settings rules are read from every file Claude Code reads") {
            let base = FileManager.default.temporaryDirectory.appendingPathComponent("rules-\(UUID().uuidString)").path
            defer { try? FileManager.default.removeItem(atPath: base) }
            func write(_ path: String, _ json: String) {
                try? FileManager.default.createDirectory(atPath: (path as NSString).deletingLastPathComponent,
                                                         withIntermediateDirectories: true)
                FileManager.default.createFile(atPath: path, contents: Data(json.utf8))
            }
            write(base + "/managed.json", #"{"permissions":{"deny":["Bash(shutdown:*)"]}}"#)
            write(base + "/config/settings.json", #"{"permissions":{"ask":["Bash(git push:*)"],"allow":["Bash(ls)"]}}"#)
            write(base + "/proj/.claude/settings.json", #"{"permissions":{"deny":["Read(./secrets/**)"]}}"#)
            write(base + "/proj/.claude/settings.local.json", "not json")
            let rules = PermissionRules.read(configHome: base + "/config", cwd: base + "/proj", managed: [base + "/managed.json"])
            suite.expect(rules.rules == ["Bash(shutdown:*)", "Bash(git push:*)", "Read(./secrets/**)"], "deny and ask, not allow: \(rules.rules)")
            let secret = permission(tool: "Read", text: base + "/proj/secrets/key.pem")
            suite.expect(PermissionRisk.assess(secret, cwd: base + "/proj", rules: rules) == .rule, "project deny rule")
            let ls = permission(tool: "Bash", text: "ls")
            suite.expect(PermissionRisk.assess(ls, cwd: base + "/proj", rules: rules) == .safe, "allow rules change nothing")
        }

        suite.run("permission risk: shell reading") {
            let parts = ShellScript.parse("a 'b; c' && d \"$(e f)\" | g > out 2>&1 < in")
            suite.expect(parts.map { $0.words.first ?? "" } == ["a", "d", "e", "g"], "segments \(parts.map(\.words))")
            suite.expect(parts.first?.words == ["a", "b; c"], "single quotes keep separators")
            suite.expect(parts.last?.piped == true && parts.last?.redirects == ["out"], "pipe and redirect on the last part")
            suite.expect(!(parts.last?.words.contains("in") ?? true), "input file is not an argument")
            suite.expect(ShellScript.name("/usr/bin/RM") == "rm" && ShellScript.name("C:\\bin\\curl.exe") == "curl", "names")
        }
    }
}
