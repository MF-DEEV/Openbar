// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// History responses replace the prior history; only notifications that raced
/// with the read should survive outside the returned items.
enum CodexConversationReviewTests {
    static func run(_ suite: TestSuite) {
        let state = CodexSessionState()
        state.setPreviews(true)
        let now = Date(timeIntervalSince1970: 1_792_600_000)
        func history(_ ids: [String]) {
            state.upsertThread([
                "id": "t", "cwd": "/tmp/app", "status": ["type": "idle"],
                "turns": [
                    [
                        "startedAt": now.timeIntervalSince1970,
                        "items": ids.map {
                            ["id": $0, "type": "agentMessage", "text": $0]
                        },
                    ]
                ],
            ])
        }
        history(["old"])
        history(["new"])
        let replaced = state.snapshot(now: now, previews: true).sessions.first
        suite.expect(
            replaced?.chat.map(\.text) == ["new"] && replaced?.lastText == "new",
            "fresh authoritative history cannot append superseded history after the latest answer")
        state.notify(
            "item/completed",
            params: [
                "threadId": "t",
                "item": [
                    "id": "live", "type": "agentMessage", "text": "Live reply", "phase": "final_answer",
                ],
            ], now: now)
        history(["new"])
        suite.expect(
            state.snapshot(now: now, previews: true).sessions.first?.chat.map(\.text) == [
                "new", "Live reply",
            ],
            "a notification received outside the returned history remains visible")
        let requestedAt = state.chatRevision
        state.upsertThread(
            [
                "id": "t",
                "turns": [
                    [
                        "items": [
                            ["id": "newer", "type": "agentMessage", "text": "Newer history"]
                        ]
                    ]
                ],
            ], historyRevision: requestedAt)
        suite.expect(
            state.snapshot(now: now, previews: true).sessions.first?.lastText == "Newer history",
            "a live reply preceding a new history request cannot become the latest answer again")
        state.notify(
            "item/started",
            params: [
                "threadId": "t",
                "item": [
                    "id": "partial", "type": "agentMessage", "text": "Working",
                ],
            ], now: now)
        let afterPartial = state.chatRevision
        state.upsertThread(
            [
                "id": "t",
                "turns": [
                    [
                        "items": [
                            [
                                "id": "partial", "type": "agentMessage", "text": "Finished",
                                "phase": "final_answer",
                            ]
                        ]
                    ]
                ],
            ], historyRevision: afterPartial)
        let completed = state.snapshot(now: now, previews: true).sessions.first
        suite.expect(
            completed?.lastText == "Finished" && completed?.chat.last?.endsTurn == true,
            "history requested after a partial live item can complete that item")
        let newer: [String: Any] = [
            "id": "t",
            "turns": [
                [
                    "items": [
                        [
                            "id": "partial", "type": "agentMessage", "text": "Newest history",
                            "phase": "final_answer",
                        ]
                    ]
                ]
            ],
        ]
        let older: [String: Any] = [
            "id": "t",
            "turns": [
                [
                    "items": [
                        ["id": "partial", "type": "agentMessage", "text": "Old partial"]
                    ]
                ]
            ],
        ]
        state.upsertThread(newer, historyRevision: afterPartial, historyOrder: 20)
        state.upsertThread(older, historyRevision: 0, historyOrder: 10)
        suite.expect(
            state.snapshot(now: now, previews: true).sessions.first?.lastText == "Newest history",
            "an older history response cannot overwrite an accepted newer response")
        state.upsertThread(older, historyRevision: afterPartial, historyOrder: 15)
        suite.expect(
            state.snapshot(now: now, previews: true).sessions.first?.lastText == "Newest history",
            "request order protects history even without intervening live messages")
    }
}
