# Claude sessions spike

Findings from a throwaway interactive Claude Code 2.1.306 session, driven through a pseudo-terminal with hooks installed only in that folder's `.claude/settings.local.json`. They ground the Sessions section of the island. The formats are internal to Claude Code and can change between versions.

## Session registry

`~/.claude/sessions/<pid>.json` appears once the session is ready and is removed when it exits normally. `status` moved through `idle`, `busy` and `waiting` as expected. A pending tool permission writes `"status":"waiting","waitingFor":"permission prompt"`, and answering it returns the file to `busy`, then `idle`.

A session started from inside another Claude Code session inherits `CLAUDE_CODE_CHILD_SESSION` and writes neither a registry file nor a transcript. Such sessions cannot be listed.

## Hooks

Every hook ran with the Claude Code process as its parent (`getppid()` is the session pid), and received `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_CODE_SSE_PORT` (when an IDE is attached) and `TERM_PROGRAM`.

For one Bash call the events arrived in this order: `PreToolUse` (with `tool_use_id`), `PermissionRequest` (no `tool_use_id`, with `permission_suggestions`), `Notification` (`permission_prompt`), `PostToolUse` (same `tool_use_id`), `Stop` (with `last_assistant_message`).

`PermissionRequest` races the terminal dialog:

- `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}` ran the command. The transcript recorded `permissionDecision: {"decision":"accept","source":"hook"}` and the terminal showed "Allowed by PermissionRequest hook".
- `behavior: "deny"` with a `message` blocked it (`"decision":"reject","source":"hook"`).
- Answering in the terminal first ran the command (`"source":"user_temporary"`), but the hook process kept waiting. The app must release a pending request itself once the registry leaves `waiting` or a later event arrives for that session.

Editing the hook settings while the session was running took effect on the next prompt, without a restart.

## Typing into the session

Text written to the session's terminal input arrives as a typed prompt (`origin: {"kind":"human"}`, `promptSource: "typed"`):

- Wrapped in bracketed paste (`ESC[200~ … ESC[201~`) and followed by a return, multi-line text became one prompt with its line breaks intact.
- Typed while the session was busy, it was queued (`queue-operation` `enqueue`), then folded into the running turn as a `queued_command` attachment and answered after it.

Text must never be typed while the registry says `waiting`: keys would go to the dialog.

## Confirmed end to end in the app

With the Developer build running against real sessions on macOS 26 and Claude Code 2.1.306:

- Installing the hooks from Settings added them to `~/.claude/settings.json` with a backup and left every other setting as it was. Sessions already running picked them up without a restart.
- A reply typed in the island reached a session in Terminal (through `do script` with a bracketed paste) and one in Antigravity's integrated terminal (through the bundled extension's `sendText`) as a typed prompt, `origin: {"kind":"human"}`.
- Allow and Deny from the island were recorded as `"source":"hook"`. Answering in the terminal first was recorded as `"user_temporary"`, and the island let go of its waiting hook at once.
- Quitting the app while a permission waited ended the hook quietly; the terminal kept asking and the answer given there went through.
- With the island closed, a session asking for permission opened it on that session's page; a finished turn showed a notice instead.
- Open selected the session's Terminal tab.

The extension's `net` server must allow half-open connections: `nc` and other clients end their side once the request is sent and still wait for the answer.

Not run end to end: the Always answer, which hands back `permission_suggestions` as `updatedPermissions` and is covered by unit tests only.
