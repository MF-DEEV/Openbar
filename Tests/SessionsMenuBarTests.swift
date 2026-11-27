// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import CoreGraphics
import Foundation

/// Sessions in the menu bar (#331): where they show, what the island keeps,
/// the keys, the panel's place and the asks in the item's menu.
enum SessionsMenuBarTests {
    static func run(_ suite: TestSuite) {
        suite.run("menu bar sessions: where they show") { placement(suite) }
        suite.run("menu bar sessions: keys answer the request on screen") { keys(suite) }
        suite.run("menu bar sessions: panel under its item") { frames(suite) }
        suite.run("menu bar sessions: count and menu") { menu(suite) }
    }

    static func defaults(island: Bool, sessions: Bool = true, placement: NotchSessionsPlacement?,
                         hidden: String = "", notchFeature: Bool = true, agents: Bool = true) -> UserDefaults {
        let name = "com.openbar.tests.menu-bar.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        defaults.set(notchFeature, forKey: AppFeature.notch.availabilityKey)
        defaults.set(agents, forKey: AppFeature.notchAgents.availabilityKey)
        defaults.set(island, forKey: DefaultsKey.notchEnabled)
        defaults.set(sessions, forKey: DefaultsKey.notchSessionsEnabled)
        defaults.set(hidden, forKey: DefaultsKey.notchHiddenModules)
        if let placement { defaults.set(placement.rawValue, forKey: DefaultsKey.notchSessionsPlacement) }
        return defaults
    }

    static func placement(_ suite: TestSuite) {
        typealias S = NotchClaudeSessionsSupport
        let unset = defaults(island: true, placement: nil)
        suite.expect(S.placement(in: unset) == .island, "nobody chose: the island, as before")
        suite.expect(S.inIsland(in: unset) && !S.inMenuBar(in: unset) && S.isEnabled(in: unset), "the island alone")
        unset.set("somewhere", forKey: DefaultsKey.notchSessionsPlacement)
        suite.expect(S.placement(in: unset) == .island, "a value it does not know reads as the island")

        let menuBar = defaults(island: true, placement: .menuBar)
        suite.expect(!S.inIsland(in: menuBar) && S.inMenuBar(in: menuBar) && S.isEnabled(in: menuBar),
                     "menu bar only: the island has no sessions")
        suite.expect(!NotchSupport.modules(in: menuBar).contains(.sessions), "no Sessions page, strip or tile in the island")
        suite.expect(!NotchSupport.routes(.sessions, in: menuBar), "and no session notices from it")

        let islandOff = defaults(island: false, placement: .menuBar)
        suite.expect(S.inMenuBar(in: islandOff) && S.isEnabled(in: islandOff),
                     "the menu bar keeps the sessions with the island turned off")
        let bothOff = defaults(island: false, placement: .both)
        suite.expect(!S.inIsland(in: bothOff) && S.inMenuBar(in: bothOff), "both, island off: the menu bar alone")
        let both = defaults(island: true, placement: .both)
        suite.expect(S.inIsland(in: both) && S.inMenuBar(in: both), "both shows them twice over")
        suite.expect(NotchSupport.modules(in: both).contains(.sessions), "both keeps the island's page")
        let islandOnlyOff = defaults(island: false, placement: .island)
        suite.expect(!S.isEnabled(in: islandOnlyOff), "island only, island off: nothing follows the sessions")

        let switchedOff = defaults(island: true, sessions: false, placement: .menuBar)
        suite.expect(!S.inMenuBar(in: switchedOff) && !S.isEnabled(in: switchedOff), "the section's switch turns both off")
        let hidden = defaults(island: true, placement: .both, hidden: "music,sessions")
        suite.expect(!S.inMenuBar(in: hidden) && !S.inIsland(in: hidden), "hidden in the island's list means off")
        let noAgents = defaults(island: false, placement: .menuBar, agents: false)
        suite.expect(!S.inMenuBar(in: noAgents), "without the AI feature there are no sessions to show")
        let noIslandFeature = defaults(island: false, placement: .menuBar, notchFeature: false)
        suite.expect(!S.inMenuBar(in: noIslandFeature), "the island's feature holds the setting, so it holds the menu bar too")

        suite.expect(NotchSessionsPlacement.island.inIsland && !NotchSessionsPlacement.island.inMenuBar
                     && !NotchSessionsPlacement.menuBar.inIsland && NotchSessionsPlacement.menuBar.inMenuBar
                     && NotchSessionsPlacement.both.inIsland && NotchSessionsPlacement.both.inMenuBar,
                     "each choice's two halves")
    }

    static func keys(_ suite: TestSuite) {
        typealias Q = NotchPermissionQueueTests
        let a = Q.session("a", [Q.ask("a1", 5)])
        let b = Q.session("b", [Q.ask("b1", 1, always: true)])
        var question = Q.ask("c1", 0, tool: ClaudeQuestion.tool)
        question.questions = [ClaudeQuestion(question: "Which?", header: nil, options: [], multiSelect: false)]
        let c = Q.session("c", [question])
        let sessions = [a, b, c]
        let target = { (decision: ClaudeHookMessage.Decision, focused: ClaudeSession?) in
            NotchClaudeSessionsSupport.keyTarget(decision, focused: focused, sessions: sessions, dressing: false)
        }
        suite.expect(target(.allow, nil)?.request == "b1", "the list: the oldest ask that is not a question")
        suite.expect(target(.allow, a)?.request == "a1", "a page: its own card")
        suite.expect(target(.allowAlways, nil) == nil, "Always only on a page's card")
        suite.expect(target(.allowAlways, b)?.request == "b1" && target(.allowAlways, a) == nil,
                     "Always where the card offers it")
        suite.expect(target(.allow, c) == nil && target(.deny, c)?.request == "c1", "a question: only ⌘N skips it")
        suite.expect(NotchClaudeSessionsSupport.keyTarget(.allow, focused: a, sessions: sessions, dressing: true) == nil,
                     "the wardrobe covers the card")
        suite.expect(NotchClaudeSessionsSupport.keyTarget(.allow, focused: nil, sessions: [c], dressing: false) == nil,
                     "only a question waiting: nothing for ⌘Y")
    }

    static func frames(_ suite: TestSuite) {
        typealias M = SessionsMenuBarSupport
        let visible = CGRect(x: 0, y: 0, width: 1512, height: 944)
        let item = CGRect(x: 1200, y: 944, width: 30, height: 38)
        let frame = M.frame(size: M.panelSize, under: item, visible: visible)
        suite.expect(frame.midX == item.midX && frame.maxY == item.minY - M.gap && frame.size == M.panelSize,
                     "centered under the item, a few points below the bar: \(frame)")
        let corner = M.frame(size: M.panelSize, under: CGRect(x: 1490, y: 944, width: 22, height: 38), visible: visible)
        suite.expect(corner.maxX == visible.maxX - M.margin, "kept inside the screen's right edge: \(corner)")
        let left = M.frame(size: M.panelSize, under: CGRect(x: 4, y: 944, width: 22, height: 38), visible: visible)
        suite.expect(left.minX == visible.minX + M.margin, "and its left edge: \(left)")
        let lost = M.frame(size: M.panelSize, under: nil, visible: visible)
        suite.expect(lost.maxX == visible.maxX - M.margin && lost.maxY == visible.maxY - M.gap,
                     "without a frame to trust, at the right end of the menu bar: \(lost)")
        let small = CGRect(x: 0, y: 0, width: 400, height: 300)
        let fit = M.frame(size: M.enlargedSize, under: CGRect(x: 200, y: 300, width: 22, height: 24), visible: small)
        suite.expect(fit.width == 400 - M.margin * 2 && fit.minY >= small.minY + M.margin && fit.maxY <= 300 - M.gap,
                     "an enlarged panel shrinks to a small screen: \(fit)")
        let content = M.contentSize(in: M.panelSize)
        suite.expect(content.width == M.panelSize.width - M.horizontalInset * 2 && content.height > 300,
                     "the page keeps most of the panel: \(content)")
    }

    static func menu(_ suite: TestSuite) {
        typealias Q = NotchPermissionQueueTests
        typealias M = SessionsMenuBarSupport
        suite.expect(M.countTitle(0) == "" && M.countTitle(1) == "" && M.countTitle(2) == "2" && M.countTitle(12) == "12",
                     "the dot tells one, the count from two")
        var question = Q.ask("q1", 3, tool: ClaudeQuestion.tool)
        question.questions = [ClaudeQuestion(question: "Which?", header: nil, options: [], multiSelect: false)]
        let sessions = [
            Q.session("a", [Q.ask("a1", 2, text: "rm -rf build", risk: .deletes), Q.ask("a2", 6, text: "ls")]),
            Q.session("b", [Q.ask("b1", 1, tool: ClaudeToolSummary.planTool, text: "# Plan")]),
            Q.session("c", [question]),
            Q.session("d", [Q.ask("d1", 4, text: "echo hi", complete: false)]),
            Q.session("e", [Q.ask("e1", 5, text: "npm test", risk: .unassessed)]),
        ]
        let asks = M.menuAsks(sessions) { "Name \($0.id)" }
        suite.expect(asks.map(\.request) == ["b1", "a1", "q1", "d1", "e1", "a2"], "oldest first, as the queue: \(asks.map(\.request))")
        let byID = Dictionary(uniqueKeysWithValues: asks.map { ($0.request, $0) })
        suite.expect(byID["a1"]?.decides == true && byID["a2"]?.decides == true && byID["e1"]?.decides == true,
                     "Allow and Deny where the queue answers in place")
        suite.expect(byID["b1"]?.decides == false && byID["q1"]?.decides == false && byID["d1"]?.decides == false,
                     "plans, questions and commands to read whole only open")
        suite.expect(byID["a1"]?.title == "Name a — rm -rf build", "the session, then the command: \(byID["a1"]?.title ?? "")")
        suite.expect(byID["a1"]?.risk == .deletes && byID["a2"]?.risk == nil && byID["e1"]?.risk == nil,
                     "a risk the queue found shows; one not checked yet does not")
        let long = String(repeating: "x", count: 120)
        suite.expect(M.menuTitle(long).count == 72 && M.menuTitle(long).hasSuffix("…"), "long asks end in an ellipsis")
        suite.expect(M.menuTitle("first\nsecond") == "first", "one line")
    }
}
