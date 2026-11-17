// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation

enum PermissionInspectionTests {
    static let now = Date(timeIntervalSince1970: 1_800_000_000)
    static func permission(_ raw: String, complete: Bool = true, request: String = "A") -> ClaudePermission {
        let content = PermissionContent(full: raw, complete: complete, originalLength: raw.unicodeScalars.count)
        return ClaudePermission(requestID: request, tool: "Bash", summary: nil, detail: content.preview,
                                content: content, offersAlways: true, since: now)
    }
    static func run(_ suite: TestSuite) {
        suite.run("permission security: boundaries and exact inspection") {
            for n in [0, 1, 3999, 4000, 4001, 7999, 8000, 8001, 40000] {
                let raw = String(repeating: "x", count: n)
                let p = permission(raw)
                let gate = PermissionInspectionGate()
                suite.expect(p.content?.full == raw, "authoritative text preserves all \(n) characters")
                suite.expect(gate.allows("s", p, revision: p.content!.revision(p), token: nil) == (n <= 8000), "preview boundary \(n)")
            }
            let raw = String(repeating: "# harmless\n", count: 1000) + "\nprintf 'HIDDEN_SUFFIX_FIXTURE'"
            let p = permission(raw)
            let gate = PermissionInspectionGate()
            let revision = p.content!.revision(p)
            suite.expect(!p.detail.contains("HIDDEN_SUFFIX_FIXTURE"), "fixture suffix is beyond preview")
            suite.expect(!gate.allows("s", p, revision: revision, token: "forged"), "programmatic/keyboard bypass rejected")
            let shown = gate.open("s", p, revision: revision)!
            suite.expect(shown.text == raw, "full viewer receives every character including hidden suffix")
            suite.expect(gate.allows("s", p, revision: revision, token: shown.token), "exact inspected request can be allowed")
            suite.expect(!gate.allows("other", p, revision: revision, token: shown.token), "another session cannot inherit inspection")
            suite.expect(!gate.allows("s", permission(raw, request: "B"), revision: nil, token: shown.token), "replacement request stays blocked")
            suite.expect(!gate.allows("s", permission(raw + "\necho changed"), revision: nil, token: shown.token), "changed command invalidates inspection")
            gate.close(shown.token)
            suite.expect(!gate.allows("s", p, revision: revision, token: shown.token), "closing the viewer revokes inspection")
            let a = gate.open("a", p, revision: revision)!
            let b = gate.open("b", p, revision: revision)!
            gate.invalidate("a")
            suite.expect(!gate.allows("a", p, revision: revision, token: a.token)
                         && gate.allows("b", p, revision: revision, token: b.token), "simultaneous sessions remain independent")
            gate.retain([])
            suite.expect(!gate.allows("b", p, revision: revision, token: b.token), "expiry/disconnection clears inspection")
        }
        suite.run("permission security: empty requests, emoji and wrapped Codex commands") {
            let base = #""session_id":"s1","transcript_path":"/x/s1.jsonl","cwd":"/a""#
            func asked(_ tool: String, _ input: String) -> ClaudePermission? {
                let json = #"{\#(base),"hook_event_name":"PermissionRequest","tool_name":"\#(tool)","tool_input":\#(input)}"#
                let message = ClaudeHookMessage.decode(Data(json.utf8), requestID: "r1", now: now, policy: ClaudeContentPolicy(previews: false))
                guard case .permissionRequested(let ask)? = message?.fact.event else { return nil }
                return ask
            }
            let gate = PermissionInspectionGate()
            if let tool = asked("mcp__notes__list", "{}"), let content = tool.content {
                suite.expect(!content.requiresInspection && gate.allows("s", tool, revision: content.revision(tool), token: nil),
                             "a tool with no arguments is allowed as it stands")
            } else { suite.expect(false, "a tool with no arguments decodes") }
            if let bash = asked("Bash", #"{"command":""}"#), let content = bash.content {
                suite.expect(content.requiresInspection && !gate.allows("s", bash, revision: content.revision(bash), token: nil),
                             "an empty command stays blocked")
            } else { suite.expect(false, "an empty command decodes") }
            let emoji = permission("echo 'dev 🧑‍💻 👩🏽‍🚀 🏳️‍🌈'")
            suite.expect(!emoji.content!.requiresInspection && !emoji.detail.contains("⟦"), "a joiner inside an emoji is not hidden text")
            for raw in ["echo a\u{200D}b", "echo 🧑\u{200D}", "\u{200D}💻", "echo 🧑\u{200D}x"] {
                suite.expect(permission(raw).content!.requiresInspection, "a joiner outside an emoji is annotated: \(raw.debugDescription)")
            }
            let script = CodexSessionState.approvalScript
            suite.expect(script("/bin/zsh -lc 'npm test'") == "npm test" && script("bash -l -c 'a\nb'") == "a\nb",
                         "a system shell's script is shown alone")
            suite.expect(script(#""C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe" -Command 'echo ''hi'''"#) == "echo 'hi'"
                         && script(#"pwsh.exe -NoProfile -Command "dir""#) == "dir" && script("cmd.exe /c dir") == "dir",
                         "PowerShell and cmd scripts are shown alone")
            for hiding in [#"/bin/bash -c 'eval "$1"' _ 'rm -rf ~'"#, "/tmp/x/bash -lc 'ls'", "bash --rcfile=/tmp/x -c 'ls'",
                           #"zsh -lc $'\x72m -rf ~'"#, "evil; powershell.exe -Command 'ls'", "powershell.exe -Command 'a' ; 'b'",
                           "powershell.exe -Command 'ls'; rm x", "echo cmd /c dir", "ls"] {
                suite.expect(script(hiding) == nil, "a wrapper that could hide something is shown whole: \(hiding)")
            }
        }
        suite.run("permission security: whitespace Unicode controls and resource limits") {
            let raw = "  printf '%s\\n' 'zażółć 🦀'\r\n\t echo \"quoted\"\u{1b}[2J\u{202e}  "
            let p = permission(raw)
            suite.expect(Array(p.content!.full.utf8) == Array(raw.utf8), "original bytes, whitespace, Unicode, quoting, escapes and controls are intact")
            let decomposed = "e\u{0301}"
            suite.expect(Array(PermissionContent.display(decomposed).utf8) == Array(decomposed.utf8), "display does not normalize Unicode")
            let composedCard = permission("\u{e9}")
            let decomposedCard = permission(decomposed)
            suite.expect(composedCard.content != decomposedCard.content, "snapshots refresh on canonically equivalent but byte-distinct commands")
            suite.expect(composedCard.content!.revision(composedCard) != decomposedCard.content!.revision(decomposedCard), "byte changes invalidate inspection even for canonically equivalent text")
            let prefix = String(repeating: "x", count: 9000)
            suite.expect(ClaudeToolSummary.inputKey(name: "Bash", input: ["command": prefix + "A"])
                         != ClaudeToolSummary.inputKey(name: "Bash", input: ["command": prefix + "B"]), "matching tool inputs includes the entire suffix")
            suite.expect(p.detail.contains("⟦U+001B⟧") && p.detail.contains("⟦U+202E⟧"), "controls are visibly annotated")
            let gate = PermissionInspectionGate()
            let incomplete = permission("partial", complete: false)
            suite.expect(gate.open("s", incomplete, revision: incomplete.content!.revision(incomplete)) == nil, "incomplete text cannot be inspected as complete")
            suite.expect(!gate.allows("s", incomplete, revision: nil, token: nil), "incomplete text cannot be allowed")
            let huge = permission(String(repeating: "x", count: PermissionContent.viewerBytes + 1))
            suite.expect(!huge.content!.available && !gate.allows("s", huge, revision: nil, token: nil), "viewer resource failure blocks approval")
        }
        suite.run("permission security: custom OpenCode patterns require provenance") {
            let raw = "\n  " + String(repeating: "x", count: 9000) + " HIDDEN_PATTERN_SUFFIX \u{1f980}  \n"
            for complete in [false, true] {
                let envelope = AgentHookSessionTests.envelope("opencode", [
                    "kind": "permission", "session": "s", "id": "pattern", "permission": "custom",
                    "patterns": ["", String(raw.dropFirst().dropLast()), ""], "metadata": [:],
                    "permissionContentComplete": complete, "permissionDetailLength": raw.unicodeScalars.count,
                ])
                if case .permission(let p)? = AgentHookSessionTests.decode(envelope).first?.kind {
                    suite.expect(p.content?.full == raw && p.content?.complete == complete, "pattern contents and source completeness are preserved")
                    let gate = PermissionInspectionGate()
                    let revision = p.content!.revision(p)
                    suite.expect(!gate.allows("opencode:s", p, revision: revision, token: nil), "long patterns require inspection")
                    let proof = gate.open("opencode:s", p, revision: revision)
                    suite.expect((proof != nil) == complete, "legacy/custom incomplete content cannot unlock approval")
                    if let proof {
                        suite.expect(proof.text == raw && gate.allows("opencode:s", p, revision: revision, token: proof.token), "inspected pattern matches the exact request")
                    }
                } else { suite.expect(false, "custom OpenCode permission decoded") }
            }
        }
        suite.run("permission security: Claude Codex and OpenCode integrations") {
            let raw = "  " + String(repeating: "x", count: 12000) + "\n# suffix 🦀  "
            let payload: [String: Any] = ["session_id": "s", "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": ["command": raw]]
            let data = try! JSONSerialization.data(withJSONObject: payload)
            let message = ClaudeHookMessage.decode(data, requestID: "r", now: now, policy: ClaudeContentPolicy(previews: false))!
            if case .permissionRequested(let p) = message.fact.event {
                suite.expect(p.content?.full == raw, "Claude full command is never clipped")
            } else { suite.expect(false, "Claude permission decoded") }
            let state = CodexSessionTests.stateWithThread()
            _ = state.request(.number(224), method: "item/commandExecution/requestApproval", params: ["threadId": "t1", "command": raw], now: now)
            let p = state.snapshot(now: now, previews: false).sessions.first?.permission
            suite.expect(p?.content?.full == raw, "Codex preserves the raw command without unwrapping or trimming")
            for complete in [false, true] {
                var metadata: [String: Any] = ["command": raw]
                if complete { metadata["commandComplete"] = true; metadata["commandLength"] = raw.unicodeScalars.count }
                let envelope = AgentHookSessionTests.envelope("opencode", ["kind": "permission", "session": "s", "id": "r", "permission": "bash", "metadata": metadata])
                let events = AgentHookSessionTests.decode(envelope)
                if case .permission(let permission)? = events.first?.kind {
                    suite.expect(permission.content?.full == raw && permission.content?.complete == complete,
                                 "OpenCode preserves >4000 characters; legacy payload is explicitly incomplete")
                    suite.expect(!PermissionInspectionGate().allows("opencode:s", permission, revision: nil, token: nil), "OpenCode approval requires full inspection")
                } else { suite.expect(false, "OpenCode permission decoded") }
            }
            let disguised = AgentHookSessionTests.envelope("opencode", ["kind": "permission", "session": "s", "id": "diff", "permission": "edit", "metadata": [
                "command": "echo FIXTURE", "commandComplete": true, "commandLength": 12,
                "filepath": "/fixture.txt", "diff": "--- a/fixture.txt\n+++ b/fixture.txt\n@@ -1 +1 @@\n-old\n+new\n",
            ]])
            if case .permission(let p)? = AgentHookSessionTests.decode(disguised).first?.kind {
                suite.expect(p.diffs.isEmpty && p.detail == "echo FIXTURE", "file diff never replaces the authoritative command in the card")
            } else { suite.expect(false, "mixed command and diff payload decoded") }
            let malformedMetadata: [[String: Any]] = [
                ["command": "x", "commandComplete": 1, "commandLength": 1],
                ["command": "x", "commandComplete": true, "commandLength": true],
                ["command": "x", "commandComplete": "true", "commandLength": 1],
                ["command": 42, "commandComplete": true, "commandLength": 1],
            ]
            for metadata in malformedMetadata {
                let envelope = AgentHookSessionTests.envelope("opencode", ["kind": "permission", "session": "s", "id": "r", "permission": "bash", "metadata": metadata])
                if case .permission(let p)? = AgentHookSessionTests.decode(envelope).first?.kind {
                    suite.expect(p.content?.complete == false, "malformed completeness metadata fails closed")
                } else { suite.expect(false, "malformed command still permits Deny") }
            }
        }
    }
}
