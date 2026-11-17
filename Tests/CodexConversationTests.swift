// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum CodexConversationTests {
    static func run(_ suite: TestSuite) {
        let state = CodexSessionState()
        let now = Date(timeIntervalSince1970: 1_792_600_000)
        func answer(_ id: String, _ text: String, final: Bool = false) -> [String: Any] {
            [
                "id": id, "type": "agentMessage", "text": text,
                "phase": final ? "final_answer" : "commentary",
            ]
        }
        func history(_ items: [[String: Any]]) {
            state.upsertThread([
                "id": "t", "cwd": "/tmp/app", "status": ["type": "idle"],
                "turns": [["startedAt": now.timeIntervalSince1970, "items": items]],
            ])
        }
        func shown(_ previews: Bool = true) -> ClaudeSession? {
            state.snapshot(now: now, previews: previews).sessions.first
        }
        history([answer("a", "private")])
        suite.expect(
            state.threads["t"]?.chat.isEmpty == true, "history is never retained before preview opt-in")
        let user: [String: Any] = [
            "id": "u", "type": "userMessage",
            "content": [
                ["type": "text", "text": "Hello"], ["type": "image", "url": "private"],
                ["type": "text", "text": "world"],
            ],
        ]
        state.setPreviews(true)
        history([user, answer("a", "Working")])
        let originalIDs = shown()?.chat.map(\.id)
        suite.expect(
            shown()?.chat.map(\.text) == ["Hello\nworld", "Working"],
            "history preserves order and includes text only")
        history([user, answer("a", "Working")])
        suite.expect(
            shown()?.chat.map(\.id) == originalIDs, "repeated history keeps stable message identities")
        let final = ["threadId": "t", "item": answer("a", "Done", final: true)] as [String: Any]
        suite.expect(
            state.notify("item/completed", params: final, now: now).changed,
            "live replies update existing items")
        suite.expect(
            !state.notify("item/completed", params: final, now: now).changed,
            "duplicate completion does not duplicate messages")
        history([user, answer("a", "Working")])
        suite.expect(
            shown()?.chat.count == 2 && shown()?.lastText == "Done"
                && shown()?.chat.last?.endsTurn == true,
            "late history cannot overwrite the final live answer or its Summary marker")
        state.notify(
            "item/completed", params: ["threadId": "t", "item": answer("new", "Newest")], now: now)
        history([user, answer("a", "Working")])
        suite.expect(
            shown()?.chat.map(\.text) == ["Hello\nworld", "Done", "Newest"],
            "live messages missing from history remain last")
        let hidden = shown(false)
        suite.expect(
            hidden?.chat.isEmpty == true && hidden?.lastText == nil,
            "a hidden snapshot exposes no conversation")
        for index in 0..<105 {
            state.notify(
                "item/completed", params: ["threadId": "t", "item": answer("n\(index)", "Reply \(index)")],
                now: now)
        }
        suite.expect(
            shown()?.chat.count == 100 && shown()?.lastText == "Reply 104",
            "live chat retains only its latest hundred messages")
        history((0..<120).map { answer("h\($0)", "History \($0)") })
        suite.expect(
            shown()?.chat.count == 100 && shown()?.lastText == "Reply 104",
            "large history keeps the newer live tail within the bound")
        state.notify(
            "item/completed",
            params: ["threadId": "t", "item": answer("long", String(repeating: "🎵", count: 20_000))],
            now: now)
        suite.expect(
            shown()?.lastText?.hasPrefix(String(repeating: "🎵", count: 16_000)) == true
                && (shown()?.lastText?.count ?? 0) < 16_100,
            "long Unicode replies are bounded without splitting characters")
        let count = shown()?.chat.count
        for item: [String: Any] in [
            ["id": "empty", "type": "agentMessage", "text": " \n "],
            ["type": "agentMessage", "text": "No id"],
            ["id": "unknown", "type": "unknown", "text": "Ignored"],
        ] {
            state.notify("item/completed", params: ["threadId": "t", "item": item], now: now)
        }
        suite.expect(
            shown()?.chat.count == count, "blank, malformed and unsupported items add no messages")
        state.setPreviews(false)
        history([answer("late", "private")])
        state.notify("item/completed", params: final, now: now)
        suite.expect(
            state.threads["t"]?.chat.isEmpty == true,
            "late history and live events cannot restore content after opt-out")
        state.setPreviews(true)
        suite.expect(
            shown()?.chat.isEmpty == true, "opting in again starts empty until fresh history arrives")
        history([answer("fresh", "Fresh")])
        suite.expect(shown()?.lastText == "Fresh", "fresh history repopulates previews after opt-in")
        state.reset()
        suite.expect(state.threads.isEmpty, "daemon disconnect discards chat with the thread")
    }

    static func images(_ suite: TestSuite) {
        let state = CodexSessionState()
        let now = Date(timeIntervalSince1970: 1_792_600_000)
        let small = ClaudeSessionReaderTests.png(width: 40, height: 30).base64EncodedString()
        let item: [String: Any] = [
            "id": "pic", "type": "userMessage",
            "content": [
                ["type": "localImage", "path": "/tmp/codex/photo.png"],
                ["type": "image", "url": "data:image/png;base64,\(small)"],
                ["type": "image", "url": "https://example.com/remote.png"],
                ["type": "localImage", "path": "relative.png"],
            ],
        ]
        func shown(_ previews: Bool = true) -> ClaudeSession? { state.snapshot(now: now, previews: previews).sessions.first }
        state.upsertThread(["id": "t", "cwd": "/tmp/app", "status": ["type": "idle"]])
        state.setPreviews(true)
        let event: [String: Any] = ["threadId": "t", "item": item]
        suite.expect(state.notify("item/completed", params: event, now: now).changed, "a prompt of images alone shows")
        suite.expect(!state.notify("item/completed", params: event, now: now).changed, "and is not added twice")
        let images = shown()?.chat.first?.images ?? []
        suite.expect(shown()?.chat.count == 1 && shown()?.chat.first?.text == "" && images.count == 2,
                     "a local file and an inline image show; a web address and a relative path never do")
        suite.expect(images.first?.source == .file("/tmp/codex/photo.png"), "the file is named, read only when shown")
        if case .preview? = images.last?.source {} else { suite.expect(false, "the inline image is kept as a preview") }
        state.upsertThread(["id": "t", "turns": [["startedAt": now.timeIntervalSince1970, "items": [item]]]])
        suite.expect(shown()?.chat.count == 1 && shown()?.chat.first?.images == images,
                     "history read again keeps the same images")
        suite.expect(shown(false)?.chat.isEmpty == true, "a hidden snapshot shows no image")
        let many: [String: Any] = [
            "id": "many", "type": "userMessage",
            "content": (0..<20).map { ["type": "localImage", "path": "/tmp/\($0).png"] },
        ]
        state.notify("item/completed", params: ["threadId": "t", "item": many], now: now)
        suite.expect(shown()?.chat.last?.images.count == ClaudeChatImage.perPrompt, "a prompt shows at most sixteen images")
        state.setPreviews(false)
        suite.expect(state.threads["t"]?.chat.isEmpty == true, "turning previews off lets the images go")
    }
}
