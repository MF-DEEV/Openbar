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

## Later findings (2.1.307)

- The registry says `"shell"` for a session at rest while a command it started in the background still runs (`status === "idle" && <background shell> ? "shell"`). The island shows it as working in the background, not done.
- While a subagent sent to the background works, the registry stays `"busy"` (`isLoading || delegatedActive`) though the turn that sent it ended with `end_turn` and the Stop hook (2.1.308). Its `Agent` result carries `toolUseResult` `isAsync: true`, `status: "async_launched"`, `agentId` and `description`, and its end comes as a `<task-notification>` naming that `agentId`. The island shows the session in the background until then; the registry going idle releases it if the end was missed.
- The registry rests (`"idle"`, or `"shell"` with only a command of its own running) only once no subagent works, so the island ends every subagent last seen working before that change, even one whose end it never read (verified 2.1.312).
- Claude Code's own forks (`runForkedAgent`: `extract_memories` every few turns, `prompt_suggestion`, `hook_prompt` and others) get a fresh `agentId`, so their tool hooks carry `agent_id`, but they fire no `SubagentStart` or `SubagentStop` (only `runAgent` does) and keep no transcript (`skipTranscript`). A hook from inside an agent the island has not seen start lists nothing (2.1.312).
- `"dialog open"` covers every dialog in the terminal, including ones the person opens, like `/usage`. Only dialogs Claude Code raises itself send a Notification hook.
- Claude Code attaches an image when a paste is nothing but the path of a PNG, JPEG, GIF or WebP file, and reads the file a moment later. A return sent at once goes out without it. Terminal adds a return after every `do script`, so the image paths go first with a paste left open, which that return joins as a line break; after a pause the text closes the paste and its return sends the reply. Confirmed in a throwaway session: the transcript held the image block and the model described the picture.

## Questions (2.1.308)

- AskUserQuestion runs the PermissionRequest hook like any tool, with `tool_input.questions` as Claude wrote them. Answering `{"behavior":"allow","updatedInput":{<the same input>,"answers":{"<question text>":"<label>"}}}` answers it: the terminal shows "User answered Claude's questions … Allowed by PermissionRequest hook". A multiSelect question takes a list of labels, and any text not among the labels stands as the person's own answer.
- Claude Code checks the answer: every other key must equal the input it sent, keys of `answers` must be its question texts, a list is taken only for multiSelect, and all answers together must stay within 32,768 UTF-16 units.
- While the dialog is open the registry says `waitingFor: "input needed"`, not `"permission prompt"`.
- Confirmed in throwaway sessions: a single choice clicked on the island, two questions with a multiSelect answered together, and Skip, which Claude reads as "The user skipped the question from the Dynamic Island without answering."

## Clawds (2.1.308)

- Claude Code's theme names Clawd's body `clawd_body: rgb(215,119,87)`, and a subagent's color one of `red`, `blue`, `green`, `yellow`, `purple`, `orange`, `pink` or `cyan` (`*_FOR_SUBAGENTS_ONLY`). An agent definition picks it with a `color:` line in its frontmatter. Claude Code strips a byte order mark, accepts any line ending (`/^---\s*\n([\s\S]*?)---\s*\n?/`) and reads the frontmatter as YAML, so a comment after the value is not part of it.
- A subagent's `agent-<id>.meta.json` names its `agentType` but not its color. A plugin's agent is typed `<plugin>:<agent>`. Installed plugins are listed in `~/.claude/plugins/installed_plugins.json` (`{"version":2,"plugins":{"<plugin>@<marketplace>":[{"installPath":…}]}}`); plugins synced from the account sit in `~/.claude/plugins/synced/<account>/<folder>/`, naming themselves in `plugin.json` or `.claude-plugin/plugin.json`. The account's `manifest.json` lists the folder in use for each plugin, `<name>` or `<name>~g<generation>` when it gives a `generation`, and names older copies still on disk in `staleDirs`.
- `claude --resume <id>` keeps the session's id under a new process, so its Clawd stays the same. Confirmed in a throwaway session.
