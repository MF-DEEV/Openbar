// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum ClaudeAgentColorTests {
    static func run(_ suite: TestSuite) {
        suite.run("agent definitions") { definitions(suite) }
        suite.run("agent colors") { colors(suite) }
    }

    static func data(_ text: String) -> Data { Data(text.utf8) }

    static func definitions(_ suite: TestSuite) {
        let head = ClaudeAgentDefinitions.frontmatter(data("""
        ---
        name: code-reviewer
        description: "Reviews: code, for bugs"
        color: 'green'
        tools: Read, Grep
        ---
        You review code. color: red
        """))
        suite.expect(head.name == "code-reviewer" && head.color == "green",
                     "an agent's name and color come from its frontmatter, quotes and all: \(head)")
        let plain = ClaudeAgentDefinitions.frontmatter(data("name: x\ncolor: red\n"))
        suite.expect(plain.name == nil && plain.color == nil, "text that does not open with --- is not frontmatter")
        suite.expect(ClaudeAgentDefinitions.frontmatter(data("---\nname: x\ncolor: red\n")).name == nil,
                     "nor is a block that never closes")
        suite.expect(ClaudeAgentDefinitions.frontmatter(data("---\nname: x\n---\n")).color == nil,
                     "an agent without a color has none")
        let long = "---\nname: far\n" + String(repeating: "# filler\n", count: 600) + "color: red\n---\n"
        suite.expect(ClaudeAgentDefinitions.frontmatter(data(long)).color == nil,
                     "only the head of a definition is read")
        suite.expect(ClaudeAgentDefinitions.frontmatter(Data([0xff, 0xfe, 0x00])).name == nil, "nor is garbage")

        let home = URL(fileURLWithPath: "/Users/me")
        let project = ClaudeAgentDefinitions.folders(type: "reviewer", cwd: "/Users/me/code/app/src", home: home,
                                                    plugins: nil, readsProtected: false).map(\.path)
        suite.expect(project == ["/Users/me/code/app/src/.claude/agents", "/Users/me/code/app/.claude/agents",
                                 "/Users/me/code/.claude/agents", "/Users/me/.claude/agents", "/Users/me/.config/claude/agents"],
                     "a project's agents are looked for from its folder up, then the person's: \(project)")
        let guarded = ClaudeAgentDefinitions.folders(type: "reviewer", cwd: "/Users/me/Desktop/app", home: home,
                                                    plugins: nil, readsProtected: false).map(\.path)
        suite.expect(guarded == ["/Users/me/.claude/agents", "/Users/me/.config/claude/agents"],
                     "folders macOS guards with a prompt are left alone: \(guarded)")
        let allowed = ClaudeAgentDefinitions.folders(type: "reviewer", cwd: "/Users/me/Desktop/app", home: home,
                                                    plugins: nil, readsProtected: true).map(\.path)
        suite.expect(allowed.first == "/Users/me/Desktop/app/.claude/agents" && allowed.count == 4,
                     "unless the app has Full Disk Access: \(allowed)")
        for path in ["/Users/me/Documents/x", "/Users/me/Downloads", "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/a",
                     "/Volumes/Work/repo"] {
            suite.expect(ClaudeAgentDefinitions.isProtected(path, home: home), "\(path) is guarded")
        }
        for path in ["/Users/me/code", "/Users/me/Desktopper", "/tmp/vs-spike", "/Users/me/.claude"] {
            suite.expect(!ClaudeAgentDefinitions.isProtected(path, home: home), "\(path) is not")
        }
        let outside = ClaudeAgentDefinitions.folders(type: "reviewer", cwd: "/tmp/vs-spike", home: home, plugins: nil,
                                                    readsProtected: false).map(\.path)
        suite.expect(outside.first == "/tmp/vs-spike/.claude/agents" && outside.contains("/tmp/.claude/agents")
                     && !outside.contains("/.claude/agents"),
                     "outside home, the search stops short of the disk's top: \(outside)")
        suite.expect(ClaudeAgentDefinitions.folders(type: "reviewer", cwd: "", home: home, plugins: nil, readsProtected: true)
                     .map(\.path) == ["/Users/me/.claude/agents", "/Users/me/.config/claude/agents"],
                     "with no folder, only the person's own agents")

        let plugins = data("""
        {"version":2,"plugins":{"feature-dev@official":[{"scope":"user","installPath":"/p/feature-dev/1.0"}],
         "feature-devtools@other":[{"installPath":"/p/tools"}],"broken@x":"nope"}}
        """)
        suite.expect(ClaudeAgentDefinitions.folders(type: "feature-dev:code-reviewer", cwd: "/Users/me/code", home: home,
                                                    plugins: plugins, readsProtected: false).map(\.path)
                     == ["/p/feature-dev/1.0/agents"],
                     "a plugin's agent is looked for only in that plugin's folder")
        suite.expect(ClaudeAgentDefinitions.pluginFolders(data("[]"), plugin: "x").isEmpty
                     && ClaudeAgentDefinitions.pluginFolders(nil, plugin: "x").isEmpty,
                     "an unreadable plugin list names no folder")
        suite.expect(ClaudeAgentDefinitions.agentName("feature-dev:code-reviewer") == "code-reviewer"
                     && ClaudeAgentDefinitions.agentName("Explore") == "Explore", "an agent is called without its plugin")
    }

    static func colors(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "claude-agents-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let project = home.appending(path: "code/app")
        func write(_ text: String, to folder: URL, _ name: String) {
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try? data(text).write(to: folder.appending(path: name))
        }
        write("---\nname: reviewer\ncolor: blue\n---\n", to: home.appending(path: ".claude/agents"), "reviewer.md")
        write("---\nname: tester\ncolor: pink\n---\n", to: home.appending(path: ".claude/agents"), "a.md")
        write("---\nname: reviewer\ncolor: green\n---\n", to: project.appending(path: ".claude/agents"), "mine.md")
        write("no frontmatter here", to: project.appending(path: ".claude/agents"), "plain.md")
        let plugin = home.appending(path: "plugin-install")
        write("---\nname: architect\ncolor: purple\n---\n", to: plugin.appending(path: "agents"), "architect.md")
        write("{\"plugins\":{\"feature-dev@m\":[{\"installPath\":\"\(plugin.path)\"}]}}",
              to: home.appending(path: ".claude/plugins"), "installed_plugins.json")

        let colors = ClaudeAgentColors(home: home)
        let now = Date()
        suite.expect(colors.color(type: "reviewer", cwd: project.path, now: now) == "green",
                     "the project's own agent wins over the person's of the same name")
        suite.expect(colors.color(type: "reviewer", cwd: home.appending(path: "elsewhere").path, now: now) == "blue",
                     "elsewhere the person's agent counts")
        suite.expect(colors.color(type: "tester", cwd: project.path, now: now) == "pink",
                     "an agent is found by its own name, whatever its file is called")
        suite.expect(colors.color(type: "plain", cwd: project.path, now: now) == nil,
                     "a definition without frontmatter has no color")
        suite.expect(colors.color(type: "feature-dev:architect", cwd: project.path, now: now) == "purple",
                     "a plugin's agent has its plugin's color")
        suite.expect(colors.color(type: ClaudeSubagent.untyped, cwd: project.path, now: now) == nil
                     && colors.color(type: "general-purpose", cwd: project.path, now: now) == nil,
                     "a subagent of unknown or built-in type has no color")

        write("---\nname: reviewer\ncolor: red\n---\n", to: project.appending(path: ".claude/agents"), "mine.md")
        suite.expect(colors.color(type: "reviewer", cwd: project.path, now: now.addingTimeInterval(5)) == "green",
                     "an answer is kept for a while, so definitions are not read on every tick")
        suite.expect(colors.color(type: "reviewer", cwd: project.path, now: now.addingTimeInterval(ClaudeAgentColors.keptFor + 1))
                     == "red", "and a changed definition is picked up after that")

        var session = ClaudeSession(id: "s", pid: 1, cwd: project.path, project: "app", name: "app", host: .unknown, phase: .generating,
                                    phaseSince: now, timeline: [], subagents: [
                                        ClaudeSubagent(id: "a1", type: "tester", started: now),
                                        ClaudeSubagent(id: "a2", type: "Explore", started: now),
                                    ], tokens: AgentTokens(), cost: 0, hooked: false)
        session = colors.decorate(session, now: now)
        suite.expect(session.subagents.map(\.color) == ["pink", nil], "each subagent of a session gets its color")
    }
}
