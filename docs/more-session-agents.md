# More session agents: Gemini CLI, Copilot CLI, OpenCode and Cursor

Sessions lists every coding agent the person runs, not only Claude Code and Codex (issue #123). Each agent below gets its own row in the shared list, its own mark, color and pixel figure, its count and mark in the closed island, and as much of the island's help as the agent allows. Nothing is installed until the person presses **Install** for that agent in the Sessions settings, and **Remove** takes out only what the island added.

## What each agent reaches

| Agent | Channel | Visible | Open | Approve | Reply | Details |
|---|---|---|---|---|---|---|
| Gemini CLI | hooks in `~/.gemini/settings.json` | yes | its terminal | in Gemini CLI | typed into its terminal | tools, last reply |
| Copilot CLI | a hooks file in `~/.copilot/hooks/` | yes | its terminal | in Copilot CLI | typed into its terminal | tools |
| OpenCode | a plugin in `~/.config/opencode/plugins/` | yes | its terminal | **in the island** (once, always, reject) | **in the island**, through the plugin | tools, model, tokens, cost, title, last reply |
| Cursor (app and `cursor-agent`) | hooks in `~/.cursor/hooks.json` | yes | Cursor, or `cursor-agent`'s terminal | in Cursor | `cursor-agent` in a terminal only | tools after they run, model, last reply |

Where the island can't answer a request, the card says "Answer this one in Gemini CLI." (or the agent's name) and **Open** takes the person there.

## How the events reach the island

On the Mac each hook is the system's `nc` writing the agent's input, wrapped in an envelope, to the island's socket:

```
{ printf '{"vorssaint":"gemini","hook":"BeforeTool","pid":%d,"event":' "$PPID"; cat; printf '}'; } | /usr/bin/nc -U '<socket>' 2>/dev/null; exit 0 # vorssaint-hook <bundle>
```

On Windows the hook is the app itself, `Vorssaint.exe --hook --agent gemini --event BeforeTool`, which builds the same envelope (run with `&` under PowerShell for Gemini CLI and Cursor, and with Copilot's `exec` and `args`, which need no shell). When the island is not running the hook connects to nothing, prints nothing and exits 0, so the agent carries on as if there were no hook. The island finds the agent's own process by walking up from the hook's process past the shells, which tells it when the agent quits and which terminal shows it.

OpenCode's plugin (`AgentOpenCodePlugin.swift`, the same file as `windows/src-tauri/src/sessions/opencode-plugin.js`; a Rust test keeps them alike) runs inside OpenCode and writes envelopes of its own: `created`, `status`, `tool`, `permission`, `permissionReplied`, `message` (model, tokens, cost), `reply`, `title`, `deleted`. A `permission` envelope waits on its connection for `{"reply":"once"|"always"|"reject"}`, which the plugin hands to OpenCode with `client.postSessionIdPermissionsPermissionId`. A `listen` envelope waits for `{"prompt":{"session":…,"text":…}}`, which the plugin sends with `client.session.promptAsync`. Each time the plugin reaches the island anew, as after it restarts, it tells it of its sessions again.

## What was checked, and against what

- **Gemini CLI 0.66.0** (its `docs/hooks/reference.md` and bundle): `SessionStart`, `SessionEnd`, `BeforeAgent`, `AfterAgent`, `BeforeTool`, `AfterTool`, `Notification` with `notification_type: "ToolPermission"`. A `BeforeTool` hook can deny or force a prompt but not allow past the policy, and `Notification` is observability only, so its prompts are answered in the terminal. The hook's parent process is Gemini itself. Gemini takes a return that arrives with a paste as a line break, so a reply is pasted and sent by a return of its own half a second later. When a turn ends without text, `AfterAgent` carries `"[no response text]"`, which the island does not show. Run live on the Mac, with a `BeforeModel` hook answering in place of the model.
- **Copilot CLI 1.0.105** (docs.github.com, CLI hooks reference): camelCase events with `sessionId`, `notification` with `permission_prompt`. `permissionRequest` runs before Copilot's own rules and session approvals, so holding it in the island would ask for tools Copilot would have allowed; it is not installed. The account used here has no Copilot access, so its hooks were driven with the documented payloads through the installed hook file.
- **OpenCode 1.19.5** (`@opencode-ai/plugin` and `@opencode-ai/sdk` 1.19.5, and a logging plugin): the `permission.ask` hook is not called in this version; `permission.asked` and `permission.replied` events are, and replying through the SDK client works. In the TUI no HTTP server listens, so the plugin acts through its in-process client. Run live on the Mac with a free model: listing, Allow, Deny, a reply from the island, the island restarting.
- **Cursor** (cursor.com/docs/agent/hooks): a hook that may allow or deny an action blocks it when it answers with nothing, so only hooks that only tell are installed (`sessionStart`, `sessionEnd`, `beforeSubmitPrompt`, which answers `{"continue":true}`, `afterAgentThought`, `afterAgentResponse`, `postToolUse`, `postToolUseFailure`, `stop`). Cursor's app outlives its conversations, so one quiet for 30 minutes leaves the list. Not installed here; driven with the documented payloads through the installed hooks.

## Limits

- None of these agents keeps a registry the island could read, so a Gemini CLI, Copilot CLI or Cursor session that was already running when the island started shows up at its next event. OpenCode's plugin announces its sessions itself.
- Gemini CLI says nothing when a permission prompt is answered no; the row waits until the agent's next event.
- Usage and cost for Gemini CLI and Cursor in AI Agents are out of scope (#123).
