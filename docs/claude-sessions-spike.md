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
