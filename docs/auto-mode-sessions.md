# Auto-mode sessions (#234)

A session started in its agent's no-ask mode is never held by the island. Its
permission requests are allowed as they come, with no card, no sound and no
wait, and its page lists each one under **Let through**. Questions and plans
still come to the island. The row and the page show an **auto** mark.

Settings → Dynamic Island → Content → Sessions:

- **Let auto-mode sessions run without the island**, per agent whose mode the
  island can read (Claude Code, Codex). On by default.
- **Always proceed**, per agent whose requests the island answers (Claude Code,
  Codex, OpenCode, Mistral Vibe). Off by default. Turning it on asks first and
  says what it means. With it on, every permission request of that agent is
  allowed, in any mode. Questions still come to the island.

Code: `Services/ClaudeSessions/SessionAutoMode.swift` on the Mac,
`windows/src-tauri/src/sessions/auto_mode.rs` on Windows and Linux.

## What each agent does in its no-ask mode

Checked on 2026-11-16 with Claude Code 2.1.318 (Mac, Windows, Linux) and Codex
0.164 (Mac, Windows) and 0.169 (Linux), each against a local model stand-in.
The other agents were read from their hooks and from Openbar's code, not run.

| Agent | No-ask mode | Permission event to Openbar in that mode | Before this change | Mode readable, and a switch mid-session |
| --- | --- | --- | --- | --- |
| Claude Code | `--dangerously-skip-permissions` (`bypassPermissions`) | Only for AskUserQuestion and for commands a `permissions.ask` rule names. None for Bash, Write (also into `.git` and `.claude`) or `rm -rf`. | An `ask` rule's request showed a card and held the agent until it was answered, at most 590 s, then went back to the terminal. | Yes: every hook but SessionStart carries `permission_mode`, and each prompt in the transcript carries `permissionMode`. A switch with Shift+Tab shows from the next event, usually the next prompt. |
| Codex | `--dangerously-bypass-approvals-and-sandbox` (approval policy `never`) | None. Codex refuses escalations itself. | Nothing to stop. | Yes: `thread/resume` returns `approvalPolicy`; a `/permissions` switch arrives as `thread/settings/updated` when the next turn starts. An idle thread the island has not joined since Openbar started shows no mark until it works again. |
| Codex | `--approve-for-me` (automatic review) | Codex's own reviewer decides and can still ask. | — | Not treated as no-ask. |
| Claude Code | `auto` (classifier) | Asks on purpose when its classifier holds back. | — | Not treated as no-ask. |
| Gemini CLI | `--yolo` | Openbar only hears of a prompt through `ToolPermission` notifications and cannot answer them. In yolo Gemini does not prompt. | Nothing to stop. | Not read. |
| Copilot CLI | `--allow-all-tools` | Heard through its `permission_prompt` notification only. | Nothing to stop. | Not read. |
| OpenCode | `permission` set to `allow` in its config | `permission.asked` does not fire for allowed tools. | Nothing to stop. | Not read. Always proceed answers its requests. |
| Mistral Vibe | `--auto-approve` | No approval callback. A New session's approvals go through the island's proxy. | Nothing to stop. | Not read. Always proceed answers the proxy's approvals. |
| Cursor | `--force` | Its hooks only observe. | Nothing to stop. | Not read. |
| Pi | — | Pi has no approvals. | — | — |
| Devin CLI | its own permission settings | Heard and answered in Devin. | Nothing to stop. | Not read. |
| Google Antigravity | its auto-run setting | The island's PreToolUse hook hands each call back to Antigravity's own check (`ask`). | Not verified. | Not read. |

## Notes

- A request let through is allowed for that call only. No rule is added to
  the agent's settings.
- Codex sets its "waiting on approval" flag a moment before the request
  itself arrives. For a session the island lets through, that flag counts as
  work for 3 seconds, so no waiting sound plays in between.
- The **Let through** list keeps the last 30 entries per session while
  Openbar runs.
