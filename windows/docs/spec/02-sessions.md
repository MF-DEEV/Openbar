# 02 — Claude Code Sessions: behavior and protocol spec

Status: reverse-engineered from the macOS Swift source on branch `feat/windows` (fork MF-DEEV, base commit `482540dd`).
Audience: the developer rebuilding the "Sessions" section of the Dynamic Island on Windows 11 (Tauri 2 + React/TS + Rust).
This document says what the Mac app does. Proposed Windows behavior is in §6 and marked **[WIN]**. Anything not yet tested on Windows is marked **[VERIFY]**.

**Reference convention.** File names are unique across the repo, so references are `FileName.swift:line`. Locations:

| Short name | Path |
|---|---|
| `ClaudeSession*.swift`, `ClaudeHook*.swift`, `ClaudeTranscriptReader.swift`, `ClaudeImageAttachments.swift`, `ClaudeAgentColors.swift`, `ClawdCostume.swift` | `Sources/Vorssaint/Services/ClaudeSessions/` |
| `NotchClaudeSessionsSupport.swift`, `NotchService.swift`, `NotchSupport.swift`, `NotchAgentSupport.swift` | `Sources/Vorssaint/Services/Notch/` |
| `NotchClaudeSession*View.swift`, `NotchClaudeSessionStrip.swift`, `NotchClaudeRichText.swift`, `NotchClawd*.swift`, `NotchAgentComponents.swift`, `NotchNoticeView.swift`, `NotchCapsuleViews.swift` | `Sources/Vorssaint/UI/Notch/` |
| `NotchClaudeSessionStrings.swift`, `Defaults.swift` | `Sources/Vorssaint/Core/` |
| `NotchClaudeSessionsSettingsControls.swift` | `Sources/Vorssaint/UI/Settings/` |
| `AgentUsageStore.swift`, `AgentLogParser.swift`, `Harness/ClaudeReader.swift` | `Sources/Vorssaint/Services/AgentUsage/` |
| Antigravity companion extension | `Tools/AntigravityBridge/extension/extension.js`, `package.json`, `extension.vsixmanifest` |
| Tests that pin behavior | `Tests/ClaudeHookTests.swift`, `ClaudeSessionEngineTests.swift`, `ClaudeSessionReaderTests.swift`, `ClawdCostumeTests.swift`, `NotchClawdSpriteTests.swift`, `ClaudeAgentColorTests.swift` |

`NotchMascotView.swift` is **not** part of this section. It draws the island's general robot companion, not Clawd. Leave it to the island/companion spec.

---

## 0. Architecture in one picture

```
 Claude Code process (claude, one per session)
   │ writes ~/.claude/sessions/<pid>.json   (registry: status, waitingFor, name…)
   │ writes ~/.claude/projects/<enc-cwd>/<sessionId>.jsonl  (+ <sessionId>/subagents/agent-<id>.jsonl, .meta.json)
   │ writes ~/.claude/tasks/<sessionId>/*.json  (task list)
   │ runs hooks from ~/.claude/settings.json:
   │     /usr/bin/nc -U '<app socket>' 2>/dev/null; exit 0 # vorssaint-hook <bundleId>
   ▼
 ClaudeHookServer (AF_UNIX stream socket, 0600)  ──► ClaudeSessionService (private queue)
                                                      ├─ polls registry every 1 s (active) / 3 s (quiet)
                                                      ├─ tails transcripts (appended bytes only)
                                                      ├─ ClaudeSessionEngine: joins registry + transcript facts + hook facts → phases
                                                      ├─ publishes [ClaudeSession] + events → island UI (strip, list, page, notices)
                                                      └─ actions: allow/deny/answer (hook reply), reply typing, focus window
 Reply / focus:
   Terminal.app  ← AppleScript, tab matched by tty, bracketed paste via `do script`
   Antigravity   ← companion VS Code extension, per-window socket ~/.vorssaint/terminals/<extHostPid>.sock
```

The engine is pure (no files, clocks or queues; `ClaudeSessionEngine.swift:6-16`). The service does all I/O (`ClaudeSessionService.swift:8-17`). The Windows port should keep the same split: a pure Rust `engine` module with the same rules plus unit tests ported from `ClaudeSessionEngineTests.swift`, and an I/O `service` module.

---

## 1. Data sources

### 1.1 Feature gating and lifecycle

- The section is active only if the island is on, the module list contains `.sessions`, and `notchSessionsEnabled` is true (default **false**). See `NotchClaudeSessionsSupport.swift:11-13`, `NotchSupport.swift:1859`, `Defaults.swift:1498`. It also depends on the AI feature being available (`NotchSupport.swift:79`).
- `syncWithPreferences()` (`ClaudeSessionService.swift:98-134`) is called by the island (`NotchService.swift:1152`). It:
  1. Applies the content policy (previews on/off) to the engine. Turning previews **on** throws away every transcript reader, `subagentMeta` and `taskStamps`, so every transcript is read again from byte 0 and the skipped text is filled in. Turning previews **off** purges all content right away (`ClaudeSessionEngine.swift:73-75, 172-189`), then publishes.
  2. On first start: starts the hook server, schedules the 1 s timer, runs one refresh, sweeps old image copies, auto-updates the Antigravity extension if one is installed but outdated, and checks whether the hooks are installed.
- `stop()` (`ClaudeSessionService.swift:136-162`) clears everything. Each pending permission hook is finished with **no reply**, so Claude Code falls back to its own terminal dialog. It then stops the socket (unlinking it) and gives the engine an empty record list.

### 1.2 Claude Code homes

`ClaudeSessionRegistryReader.homes()` returns `~/.claude` and `~/.config/claude`, in that order (`ClaudeSessionRegistryReader.swift:14-16`). Every lookup below (sessions, projects, tasks, ide, agents, plugins) tries both homes. `CLAUDE_CONFIG_DIR` is **not** honored (see Open questions).

### 1.3 Session registry: `<home>/sessions/<pid>.json`

Written by Claude Code itself for each running interactive process. Discovery is by listing the folder; only names that are `<int>.json` count (`ClaudeSessionRegistryReader.swift:37-39`). Other files in the folder, such as `<pid>.<hash>.key`, are ignored.

A real record (Claude Code 2.1.296, macOS):

```json
{"pid":28800,"sessionId":"e1d59516-8510-410a-8bf8-327886b4fa80","cwd":"/Users/me","startedAt":1791880185844,
 "procStart":"Tue Oct 13 08:29:44 2026","version":"2.1.296","peerProtocol":1,
 "peerFeatures":["notify_idle","reply_across_default_dirs","artifact_yield"],"kind":"interactive","entrypoint":"cli",
 "pidDomain":"darwin","messagingSocketPath":"/tmp/cc-socks/28800.sock","name":"so-metro-project-setup",
 "nameSource":"auto","nameSince":1792232826299,"updatedAt":1792830272840,"status":"idle",
 "statusUpdatedAt":1792830272840,"formerNames":[{"name":"…","until":1792232826299,"sessionId":"…"}]}
```

Waiting example (from `ClaudeSessionReaderTests.swift:19-22`):

```json
{"pid":85842,"sessionId":"99c35831","cwd":"/tmp/spike/work","startedAt":1792605026467,
 "procStart":"Wed Oct 21 17:50:25 2026","version":"2.1.306","kind":"interactive","pidDomain":"darwin",
 "messagingSocketPath":"/tmp/cc-socks/85842.sock","name":"work-1f","status":"waiting",
 "waitingFor":"permission prompt","updatedAt":1792605040000,"statusUpdatedAt":1792605039000}
```

Fields used (`ClaudeSessionRegistryReader.swift:55-72`):

| Field | Use | Rule |
|---|---|---|
| `pid` | key; must be `> 0` and fit Int32 | required |
| `sessionId` | session identity | required, non-empty |
| `cwd` | project folder, project name | required, non-empty |
| `pidDomain` | if present and `!= "darwin"`, the record is **rejected** (for example a container sharing the folder) | optional |
| `status` | `idle` \| `busy` \| `waiting` \| `shell`; any other value → nil (unknown) | optional |
| `waitingFor` | read **only when** `status == "waiting"`. Known values: `"permission prompt"`, `"dialog open"`; anything else (e.g. `"input needed"`) counts as input | optional |
| `statusUpdatedAt` (ms) → fallback `updatedAt` (ms) | `statusUpdated` | optional |
| `procStart` (string, `ps -o lstart` in **UTC**, `EEE MMM d HH:mm:ss yyyy`, runs of spaces collapsed) → fallback `startedAt` (ms) | process-identity check | optional |
| `version` | stored, not shown | optional |
| `name` (fallback: last path component of `cwd`) | session name | optional |
| `nameSource` | `"user"` (/rename), `"auto"` (made by Claude Code) | optional |

Not read: `kind`, `entrypoint`, `messagingSocketPath`, `peer*`, `formerNames`, `nameSince`.

Validation and dedup (`ClaudeSessionRegistryReader.swift:32-51, 75-82`):
- A file larger than 64 KiB, unparsable, or missing a required field is skipped. If a process with that pid is running, the pid goes into `unreadable` (it is probably being rewritten).
- **PID reuse guard.** A record counts only when a process with that pid is running **and** `|recordedStart − actualStart| ≤ 2 s`, or the recorded start is later than the actual one by less than 60 s (older records that only have `startedAt`). With no recorded start, any running pid passes. The actual start comes from `proc_pidinfo(PROC_PIDTBSDINFO)`.
- Across the two homes the first record per pid wins. The output is sorted by pid.
- Service-level smoothing (`ClaudeSessionService.swift:456-460`): a pid in `unreadable` keeps its **last good record** for that tick, so its page and clocks do not reset.
- Engine-level dedup: `records` is keyed by `sessionId`, first one wins (`ClaudeSessionEngine.swift:93`).

**Session removed** = no longer in the registry scan. The engine iterates only current records when it builds the snapshot (`ClaudeSessionEngine.swift:196`). Activity for a session that is not listed is kept for up to **120 s** after its last fact, because hooks can arrive before the registry lists a new session (`ClaudeSessionEngine.swift:20, 128-130`). The cached phase is dropped as soon as the record goes (`:131`).

### 1.4 Transcripts: `<home>/projects/<encoded-cwd>/<sessionId>.jsonl`

Lookup (`ClaudeSessionService.swift:495-509`):
1. Use the path the hooks reported (`transcript_path` in any hook payload, cached per session; `ClaudeSessionService.swift:362`) if that file exists.
2. Otherwise scan every folder under `<home>/projects/*/` for `<sessionId>.jsonl` and cache the first match. The cwd encoding is never computed, so it does not matter.

Tailing (`AgentUsageStore.swift:775-807`, cursor `AgentLogCursor`):
- The first read starts at byte **0** (the whole file), then reads only appended bytes, in 4 MiB chunks, split into complete lines. A line longer than 32 MiB is skipped.
- The cursor starts over when the inode changes, the file shrinks, or a fingerprint of the first and last 4 KiB before the offset (FNV-1a) no longer matches (the file was rewritten in place).
- Each line goes through `ClaudeTranscriptReader.parse` (`ClaudeTranscriptReader.swift:33-159`), which emits "facts".

Per line, the following is extracted (only when `json.type` parses):

| Line | Condition | Fact |
|---|---|---|
| any line containing `"type":"assistant"` | `message.usage` and a model not starting with `<` | `usage(key, tokens, cost)`. Key = `claude:<message.id>:<requestId>`; tokens from `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`, `output_tokens_details.thinking_tokens`; cost from AgentPricing (see 03-agents.md). Streamed blocks repeat usage, so the engine merges per key (`merged`, max cost) (`Harness/ClaudeReader.swift:76-96`, `ClaudeSessionEngine.swift:314-316`) |
| any | `gitBranch` non-empty and not `"HEAD"` | `branch` |
| main transcript, contains `<task-notification>`, and is a "delivering" record (`type=="queue-operation"`, or `type=="user"` with `origin.kind=="task-notification"`, or `type=="attachment"` with `attachment.type=="queued_command"`) | `<task-id>X</task-id>` and `<status>S</status>` with S ∈ {completed, failed, killed, stopped, cancelled, error}, value ≤ 128 chars | `subagentEnded(id: X)`. This also ends background Bash jobs with that task id (`ClaudeTranscriptReader.swift:46-49, 161-185`) |
| `type:"assistant"`, `isSidechain != true` | `message.model` not starting with `<` | `model` |
| ↳ each `content[]` block `tool_use` | `id`, `name` | `toolStarted(id, name, summary (previews only), started=timestamp, inputKey)` |
| ↳ `tool_use` name `Bash` with `input.run_in_background == true` | | `backgroundStarted(toolID, service: isService(command, description), label: description ?? command (previews only))` |
| ↳ `tool_use` name in {`TaskStop`, `KillShell`, `KillBash`} with `input.task_id`/`shell_id`/`bash_id` | | `subagentEnded(id: that id)` |
| ↳ `text` block | previews on | `text(clipped to 6000 chars)` |
| ↳ `message.stop_reason` ∈ {end_turn, stop_sequence, max_tokens, refusal} | failed = `isApiErrorMessage == true` or the model starts with `<` | the last text fact gets `endsTurn = true` (only for end_turn/stop_sequence and not failed); then `turnEnded(completed: !failed)` |
| `type:"user"`, `isSidechain != true`, content has `tool_result` blocks | | per result: `toolFinished(tool_use_id, failed: is_error)`; if `toolUseResult.backgroundTaskId` → `backgroundTask(toolID, task)`; if `toolUseResult.isAsync == true` or `status == "async_launched"` with `agentId` → `backgroundStarted(service:false, label: toolUseResult.description)` + `backgroundTask(toolID, agentId)`; if `toolEndsTurn == true` → `turnEnded(completed:true)` |
| `type:"user"`, no tool results, `isMeta != true`, `isCompactSummary != true` | texts = string content or text blocks | text starting `[Request interrupted by user` or `<local-command-std` → `turnEnded(completed:false)`; text starting `<command-name>` → nothing (a slash command); otherwise `turnBegan`, plus (previews only, `origin.kind != "task-notification"`, joined text not starting with `<`) `prompt(text ≤ 6000)` |
| `type:"attachment"`, `attachment.type=="queued_command"`, previews on | `attachment.prompt` (string or text blocks) | `prompt` (a prompt queued while Claude works) |
| `type:"ai-title"` | previews on, `aiTitle` | `title(≤ 280 chars)` |

Timestamps come from `timestamp` (ISO 8601 with or without fractional seconds, or epoch seconds or ms). The fallback is "now" (`AgentLogParser.swift:262-272`).

`ClaudeBackgroundKind.isService` (`ClaudeSessionModels.swift:245-261`) runs case-insensitive regexes over `description + "\n" + command`. A match marks the command as a **service**: something meant to keep running, which is not counted as work. The pattern list, verbatim:
```
\b(serve|server|servers|serving)\b   \bbackend\b   \b(dev|devserver)\s*server\b
\b(npm|yarn|pnpm|bun)\s+(run\s+)?(dev|start|serve|watch|preview)\b   \bexpo\s+start\b   \bmetro\b   \bvite\b
\bnext\s+dev\b   \bconvex\s+dev\b   \bwebpack(-dev-server|\s+serve)\b   \bstorybook\b   http\.server   --watch\b
\bwatch(ing|er)?\b   \btail\s+-[a-z]*f   \blogcat\b   \bemulator\b   \bsimulator\b   simctl\s+boot   \bngrok\b
\btunnel\b   \bnodemon\b   \buvicorn\b   \bflask\s+run\b   \brails\s+s(erver)?\b
\bdocker\s+compose\s+up\b(?!.*\s-d\b)   \blisten(s|ing)?\b
```
At most 40 background jobs are kept per session; ended jobs are dropped first (`ClaudeSessionEngine.swift:332-337, 366`).

### 1.5 Subagent transcripts and meta

Read every **3rd tick** (`ticks % 3 == 1`; `ClaudeSessionService.swift:56, 470`):
- Folder: `<transcript without .jsonl>/subagents/`. Each `*.jsonl` there is tailed like the main file (`ClaudeSessionService.swift:522-536`). The subagent id is the file name minus `.jsonl` minus a leading `agent-` (`ClaudeTranscriptReader.swift:23-27`).
- In a subagent file only `assistant` and `user` lines count. An assistant line with a terminal `stop_reason` gives `subagentEnded`; anything else gives `subagentActivity` (`ClaudeTranscriptReader.swift:187-196`).
- `agent-<id>.meta.json` sits beside it and is read **once** per path, up to 64 KiB. Real shape: `{"agentType":"Explore","description":"…","toolUseId":"toolu_…","spawnDepth":1,"requestShape":"background","requestNonInteractive":true}`. Used: `agentType` (default `"Agent"`) and `description` (previews only) (`ClaudeTranscriptReader.swift:216-221`).

### 1.6 Task list: `<home>/tasks/<sessionId>/*.json`

Read every 3rd tick, and only when the newest mtime among the files has changed. At most 500 files (`ClaudeSessionService.swift:538-556`). Real shape:

```json
{"id":"1","subject":"…","description":"…","activeForm":"…","status":"completed","blocks":[],"blockedBy":[]}
```

`status == "deleted"` is skipped. `total` = the rest, `done` = count of `completed`, `current` = `activeForm ?? subject` of the first `in_progress` (previews only). `total == 0` → no tasks row (`ClaudeTranscriptReader.swift:224-236`). If the folder disappears, the tasks are cleared.

### 1.7 IDE lock files: `<home>/ide/<port>.lock`

Used only to jump to Antigravity windows. `<port>` comes from the session's `CLAUDE_CODE_SSE_PORT` environment variable. Real shape (token redacted):

```json
{"pid":628,"workspaceFolders":["/Users/me/Projekty/Nexus-Code"],"ideName":"Antigravity IDE","transport":"ws",
 "runningInWindows":false,"authToken":"<secret>"}
```

Only `workspaceFolders` is read, and only from files ≤ 64 KiB (`ClaudeSessionHosts.swift:100-109`). Do **not** log or keep `authToken`.

### 1.8 Agent definitions (subagent colors)

`ClaudeAgentColors.swift`. This runs on publish for every subagent whose type is not `"Agent"`. Answers are cached for **30 s** per `type|cwd` key, and the cache is wiped once it holds more than 500 entries (`:184, 199-207`).

Search order (`ClaudeAgentColors.swift:59-79, 219-239`):
1. Type `plugin:agent` (contains `:`):
   - `<home>/plugins/installed_plugins.json` (≤ 1 MiB), shape `{"plugins":{"<name>@<marketplace>":[{"installPath":"…"}]}}`. For every key whose part before `@` equals `plugin`: `<installPath>/agents/`. Keys are sorted.
   - Then `<home>/plugins/synced/<account>/…`. Up to 20 accounts, sorted. If `manifest.json` = `{"plugins":[{"name":"canva"},{"name":"x","generation":5}]}`, the folders are `name` or `name~g<generation>` (generation > 1); without a manifest, every folder, the highest `~gN` first. A folder counts when `plugin.json` or `.claude-plugin/plugin.json` has `name == plugin`. Use `<folder>/agents/`.
2. Any other type: `<dir>/.claude/agents/` for dir = cwd, then its parents, stopping before the home folder or `/`. Then `<home>/agents/` for both homes.
3. In each folder: `*.md` sorted, at most 200, regular files only (never open a FIFO). Read the first 4096 bytes and parse the YAML frontmatter between the first two `---` lines (BOM stripped, any newline style). Keys `name:` and `color:` are matched case-insensitively. The value is a quoted scalar or the text before ` #`/`\t#`; anything after a closing quote other than a comment invalidates it. A match is `name == agentName` (the type without its `plugin:` prefix), or failing that, a file named `<agentName>.md` with no `name:` line.
4. macOS only: folders under the TCC-protected areas (`Desktop`, `Documents`, `Downloads`, iCloud, `Pictures`, `Movies`, `Music`, `/Volumes`) are skipped unless the app has Full Disk Access. **[WIN]** Drop this rule; Windows has no equivalent prompt.

### 1.9 Cadence

| What | When | Ref |
|---|---|---|
| Registry scan + host lookup + transcript tail + publish | timer every **1 s** while any published session is active (generating, runningTool, background) or waiting; otherwise **3 s**; leeway 200 ms | `ClaudeSessionService.swift:53-54, 442-486` |
| Subagent files, task list | every 3rd tick | `:56, 470-479` |
| Host (terminal app) per pid | once per pid, cached while the pid lives | `:462-467` |
| Hook events | as they arrive (push) | §2 |
| On Stop hook | the session's transcript is read **before** the Stop fact is applied, so a background job started in the same turn is seen before "Done" | `:371-373` |
| Pending-hook release check | every tick | `:380-389` |
| UI clocks | 1 s `TimelineView` on the list, page and large page | views |

### 1.10 Process → host (macOS)

`ClaudeSessionHosts.host(of:)` (`ClaudeSessionService.swift:585-592`, `ClaudeSessionHosts.swift`):
- Reads two variables from the claude process's environment (`sysctl KERN_PROCARGS2`): `__CFBundleIdentifier` (set by the terminal app for its shells) and `CLAUDE_CODE_SSE_PORT` (set by the IDE integration). Nothing else is kept.
- Bundle fallback: the "responsible" regular-app owner found by walking parents (`ResponsibleProcess.regularAppOwner`).
- Classification: `com.apple.Terminal` + tty → `.terminal(tty: "/dev/ttysNNN")` (tty from `e_tdev`); `com.google.antigravity-ide` → `.antigravity(port)`; any other bundle → `.app(bundle)`; none → `.unknown`.
- `acceptsReplies` is true only for `.terminal` and `.antigravity` (`ClaudeSessionModels.swift:78-83`).

---

## 2. Hooks

### 2.1 Events installed

`ClaudeHookInstaller.swift:13-19`:

| Event | Matcher | Timeout (s) |
|---|---|---|
| SessionStart, SessionEnd, UserPromptSubmit, Notification, Stop, SubagentStart, SubagentStop | none | 5 |
| PreToolUse, PostToolUse, PostToolUseFailure | `"*"` | 5 |
| PermissionRequest | `"*"` | **600** |

### 2.2 Command

`ClaudeHookInstaller.swift:35-40`:

```
/usr/bin/nc -U '<socketPath>' 2>/dev/null; exit 0 # vorssaint-hook <bundleId>
```

- The socket path is single-quoted, with `'` escaped as `'\''`.
- `# vorssaint-hook <bundleId>` is a shell comment that doubles as the **ownership marker**. `bundleId` is `com.vorssaint.utils` (official) or `com.vorssaint.utils.dev` (Developer build), so two builds can install side by side.
- `exit 0` means the hook never fails a tool call. When the app is not running, `nc` cannot connect, prints nothing, and Claude Code carries on as if there were no hook (it shows its own terminal prompt).

Socket path (`ClaudeSessionService.swift:92-96`): `~/Library/Application Support/<bundleId>/claude-hooks.sock`. If that is too long for `sun_path` (≥ 104 bytes), the fallback is `~/.vorssaint/<bundleId>.sock`.

Real installed entry:
```json
"PermissionRequest": [ {"matcher":"*","hooks":[{"type":"command",
  "command":"/usr/bin/nc -U '/Users/me/Library/Application Support/com.vorssaint.utils.dev/claude-hooks.sock' 2>/dev/null; exit 0 # vorssaint-hook com.vorssaint.utils.dev",
  "timeout":600}]} ]
```

### 2.3 settings.json edits

File: `~/.claude/settings.json` (the `.config/claude` home is **not** considered for hooks; `ClaudeHookInstaller.swift:31-33`).

- **Install** (`installing`, `:86-98`): first remove this bundle's entries, then for each event **append** a new group `{"hooks":[{"type":"command","command":C,"timeout":T}]}` (plus `"matcher":"*"` for tool events) to `hooks[event]`. Groups belonging to the user or to other tools are never touched.
- **Remove** (`removing`, `:65-84`): in every event, drop each hook entry whose `command` contains the marker `# vorssaint-hook <bundleId>`. A group left empty is dropped, an event left with no groups is dropped, and an empty `hooks` object is dropped.
- **Installed check** (`isInstalled`, `:45-52`): every one of the 11 events has some group with a hook whose `command` equals the **current** command exactly. So a changed socket path reads as "not installed", and installing again replaces the old entries.
- **Read** (`:105-123`): no file → `{}`. A whitespace-only file → `{}`. Non-JSON, a folder, an unreadable file, or a dangling symlink → error `unreadable`, and the file is left alone.
- **Atomic update with retry** (`update`, `:131-174`):
  1. Resolve symlinks (the edit is made where the file really lives).
  2. Up to 3 attempts: read mtime + contents, compute the change. **If nothing changes, return without writing** (idempotent, and no file is created just to hold `{}`).
  3. Serialize pretty-printed, keys sorted, slashes unescaped, trailing `\n`.
  4. If mtime or contents changed meanwhile (Claude Code wrote the file), retry. After 3 retries → `busy`.
  5. Backup: `settings.json.vorssaint-<unixSeconds>.bak` beside it, mode 0600. If the backup fails → `noBackup` and nothing is written. Keep the newest **3** backups (by number in the name).
  6. Write to `.settings.json.vorssaint-<UUID>` in the same folder, copy the original POSIX permissions, then `rename()` over the target.
- User-facing errors: `NotchClaudeSessionStrings.swift` `settingsUnreadable`, `settingsBusy`, `settingsNoBackup`, or the OS error, shown as "Couldn’t change the settings: %@".
- Uninstall cleanup (`ClaudeSessionService.swift:772-802`): remove this bundle's hooks and `~/.vorssaint/<bundleId>.sock`. If no sibling build is installed and the sibling has no hooks left, also uninstall the Antigravity extension and delete `~/.vorssaint/`.

### 2.4 Transport and message protocol

Server: `ClaudeHookServer.swift`.

- `AF_UNIX`, `SOCK_STREAM`. The parent folder is created 0700. Any stale socket is unlinked first. The socket is bound under `umask(077)`, then `chmod 0600`, with backlog 16 and non-blocking accept.
- Peer check: `getpeereid` uid must equal our uid, otherwise close. The peer **pid** is taken with `LOCAL_PEERPID` (used to detect a hook that died) (`:180-203`).
- **Request**: the hook (`nc`) writes the exact JSON that Claude Code gave the hook on stdin, then half-closes its write side and **keeps reading** for an answer. The server buffers up to **4 MiB** (more → close). The message is complete when the last non-whitespace byte is `}` and the buffer parses as JSON, either on EOF or as soon as it parses (`ClaudeHookMessage.swift:133-138`, `ClaudeHookServer.swift:205-229`).
- **Fast path**: on the hook queue, any payload that is not a `PermissionRequest` is answered by **closing at once** (empty response) before any processing, so a slow transcript read never holds a session (`ClaudeSessionService.swift:344-350`).
- **PermissionRequest**: the connection is kept in `pending[requestID]` (a fresh UUID) together with the session id, `permission_suggestions` and, for AskUserQuestion, the original `tool_input` (`ClaudeSessionService.swift:363-364`). The answer is written blocking with a 2 s send timeout, then the connection closes (`ClaudeHookServer.swift:74-111`). `nc` prints the answer to stdout, which becomes the hook's output.
- If the section is stopped (`timer == nil`) or the payload does not decode, close with no answer (`ClaudeSessionService.swift:357-361`).
- **Release with no answer** (each tick, `ClaudeSessionService.swift:380-389`), when any of these holds:
  - the engine no longer holds the request (answered in the terminal, turn moved on, tool finished, registry no longer waiting…)
  - the peer process is gone (`kill(pid,0)` → `ESRCH`; Claude Code kills the hook once the person answers in the terminal)
  - it has waited more than **590 s** (`600 − 10`).

  An empty response makes Claude Code use its normal flow.
- **Race**: Claude Code shows the terminal dialog **in parallel** with the PermissionRequest hook. Whichever answers first wins. That is why every release rule above exists, and why the UI must re-check that the request is still current.

### 2.5 Hook input fields used

`ClaudeHookMessage.swift:24-85`. Common: `hook_event_name` (required), `session_id` (required), `transcript_path`, `tool_name`, `tool_input`, `tool_use_id`, `agent_id`.

| Event | Extra fields | Fact |
|---|---|---|
| SessionStart | — | `sessionStarted` (no state change) |
| SessionEnd | — | `sessionEnded` → end turn (completed:false) |
| UserPromptSubmit | — | `promptSubmitted` → begin turn |
| PreToolUse | `tool_use_id` required | `toolStarted(id, name, summary (previews), started=now, inputKey)` |
| PostToolUse / PostToolUseFailure | `tool_use_id` required | `toolFinished(id, failed: event == PostToolUseFailure)` |
| PermissionRequest, `tool_name != "AskUserQuestion"` | `permission_suggestions` (array) | `permissionRequested(requestID, tool (or "Tool"), summary, detail, offersAlways = suggestions non-empty, since=now, inputKey)` |
| PermissionRequest, `AskUserQuestion` with valid questions | `tool_input.questions` | `permissionRequested(…, questions)`; `offersAlways = false`; summary = the first question clipped to 80 chars |
| PermissionRequest, `AskUserQuestion` with **unreadable** questions | — | `.other` and NOT a permission request, so the hook is closed and the question is answered in the terminal |
| Notification | `notification_type` | `notification(type)`. Any type **not** in the quiet set marks `asked = now`. Quiet set: `idle_prompt, permission_prompt, worker_permission_prompt, auth_success, auth_storage_failure, computer_use_enter, computer_use_exit, elicitation_complete, elicitation_response, agent_completed, plugin_notification, push_notification` (`ClaudeSessionEngine.swift:356-361`) |
| Stop | `last_assistant_message` (previews only, ≤ 6000) | `stopped(lastText)` → end turn (completed:true) |
| SubagentStart | `agent_id` required, `agent_type` (default `"Agent"`) | `subagentStarted` |
| SubagentStop | `agent_id` required | `subagentStopped` |
| anything else | — | `.other` |

Any event other than SubagentStart/Stop that carries `agent_id` comes **from inside that subagent**. Its tool events never list a subagent (Claude Code's own forks run tools under an agent id but never start or stop as subagents) and never change the session's current tool. Its PermissionRequest still waits on the person but gets `toolID = ""` (it is not matched to a main-agent tool) (`ClaudeSessionEngine.swift:421-437`).

Questions are parsed by `ClaudeQuestion.questions(in:)` (`ClaudeSessionModels.swift:156-171`): `questions[]` with `question` (non-blank, required), `header`, `options[]` (`label` required, `description`, `preview`; empty strings → nil), `multiSelect`. If **any** item fails to parse, the whole request counts as unreadable.

### 2.6 Tool summary, detail and matching key

`ClaudeSessionModels.swift:364-437`:
- `summary` (one line, first line of the field, trimmed, clipped to 80 chars with `…`): Bash→`command`; Read/Edit/Write/MultiEdit/NotebookEdit→the last path component of `file_path`/`notebook_path`; Grep/Glob→`pattern`; WebFetch→the URL host; WebSearch→`query`; Task/Agent→`description`; Skill→`skill`; others→`description`.
- `detail` (always kept, previews or not): Bash→the full `command`; file tools→the full path; WebFetch→the full `url`; others→`key: value` lines in sorted key order (non-string values as compact sorted JSON). Trimmed. Past **8000** chars it is cut and gets `\n… N more characters`.
- `fitsOnALine` = no questions, no `\n` in detail, `detail.count ≤ 60` → Allow/Deny buttons are shown right in the list row.
- `inputKey` = hash(name, detail), used to match a PermissionRequest to the running PreToolUse with the same input. On the Mac it is a per-launch Swift `Hasher`. **[WIN]** Any in-process hash is fine; it is never persisted.

### 2.7 Response JSON (what the hook prints)

`ClaudeHookMessage.swift:94-123`. Always `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":<body>}}`, serialized with sorted keys.

| Decision | body |
|---|---|
| Allow | `{"behavior":"allow"}` |
| Always allow | `{"behavior":"allow","updatedPermissions":<permission_suggestions exactly as received>}` |
| Deny (tool) | `{"behavior":"deny","message":"Denied from the Dynamic Island."}` |
| Skip (question) | `{"behavior":"deny","message":"The user skipped the question from the Dynamic Island without answering."}` |
| Answer (AskUserQuestion) | `{"behavior":"allow","updatedInput":{ …original tool_input unchanged…, "answers":{"<question text>":"<label or own text>","<multi question>":["A","C"]}}}` |

Answer rules:
- Answers are keyed by the **question text**. A single answer is a string; several are an array. If there is no original input to echo back, the reply is empty, which defers to the terminal.
- The service refuses Allow/Always for a question, and refuses Answer for a tool (`ClaudeSessionService.swift:248-252`).
- `ownAnswerLimit = 7500` UTF-16 units per typed answer (Claude Code refuses more than 32768 in total for at most 4×4) (`ClaudeSessionModels.swift:144-147`, clipping at `NotchClaudeSessionsSupport.swift:371-382`).

Example: the test input `{"questions":[{"question":"Which color?","header":"Color","options":[{"label":"Red","description":"Warm"},{"label":"Blue","description":"","preview":"[ blue ]"}],"multiSelect":false},{"question":"Which sizes?","header":"","options":[{"label":"S"},{"label":"M"},{"label":"L"}],"multiSelect":true}]}` answered Blue / S+L gives `updatedInput.answers = {"Which color?":"Blue","Which sizes?":["S","L"]}` (`ClaudeHookTests.swift:24-58`).

### 2.8 Decide flow

`ClaudeSessionService.swift:239-259`:
1. **Settle guard (main thread)**: the request must be the one currently shown for that session (`shownRequests[id].request == request`) and must have been on screen for at least **0.8 s** (`settleDelay`). This stops a click or ⌘Y meant for the previous request from answering a new one. `shownRequests` is updated on every publish (`:569-574`).
2. On the queue: the engine must still hold the request and the hook must still be pending. Then send the reply, remove it from pending, resolve it in the engine, mark the session acknowledged, and publish.

---

## 3. Session phases (state machine)

### 3.1 Phases

`ClaudeSessionModels.swift:8-28`:

| Phase | UI label | Meaning | isWorking | isActive | isWaiting |
|---|---|---|---|---|---|
| `generating` | Working | writing or thinking | ✓ | ✓ | |
| `runningTool` | Running | a main-agent tool is open | ✓ | ✓ | |
| `waitingPermission` | Needs permission | tool permission prompt | | | ✓ |
| `waitingInput` | Needs you | question, dialog or other prompt | | | ✓ |
| `background` | In background | the turn ended but a background job or subagent Claude waits on still runs | | ✓ | |
| `finished` | Done | the turn ended and has not been looked at (within 10 min) | | | |
| `idle` | Idle | everything else | | | |

### 3.2 Activity model (per session)

`ClaudeSessionEngine.swift:23-63`: `turnOpen`, `turnStarted`, `turnEnded`, `completed`, `registryOnly` + `startedBefore`, `lastFact` (the newest hook or transcript time), `tools` (ring of up to 24, trimmed back to 12), `subagents`, `endedAgents`, `background`, `tasks`, `usage`, `title`, `lastText`, `chat` (≤ 150), `branch`, `model`, `permissions` (FIFO, oldest shown), `hooked`, `asked`, `acknowledged`, `firstSeen`.

Core transitions:
- `begin(date)` (`:459-472`): ignored if `date < turnEnded`. If no turn is open: close the open tools, open a turn, set `turnStarted = date`. **Always** remove permissions with `since < date` (a prompt can only be typed once the dialogs before it closed). Touch.
- `end(date, completed)` (`:474-483`): ignored if a turn is open and `date < turnStarted`. Close the turn, set `turnEnded` and `completed`, remove permissions with `since ≤ date`, close the open tools. Touch.
- `start(tool)` (`:485-501`): add the tool, or fill in a missing summary/inputKey. If no turn is open and `turnEnded ≤ tool.started`, open a turn at `tool.started` (a turn whose prompt was not seen).
- `finish(id)` (`:503-513`): set `finished = max(date, started)` and `failed`. Remove any permission whose `toolID == id` (the tool ran or was refused, here or in the terminal). A failed background command that never got a task id is marked ended.
- `touch(date)`: `lastFact = max`, `registryOnly = false`.
- Permission request (`:390-401`): opens a turn if none is open. Matches `toolID` against the running main-agent tool of the same name with an equal `inputKey`, else the single running tool of that name. Replaces any entry with the same requestID and appends.
- Registry ingest (`:92-132`):
  - status ≠ waiting with `statusUpdated` → drop permissions with `since < statusUpdated`.
  - Turn open, status idle/shell, `statusUpdated > lastFact`: if `registryOnly` (busy only per the registry, nothing typed, said or run — e.g. Claude Code writing a recap), undo the turn (`turnStarted = startedBefore`), which is **not** "finished". Otherwise end it at `statusUpdated` (completed).
  - Status busy, no turn open, `statusUpdated > lastFact` → begin the turn at `statusUpdated` with `registryOnly = true`.

### 3.3 Phase derivation (evaluated in this order)

`ClaudeSessionEngine.swift:223-257`:

1. The engine holds a permission → `waitingInput` if it asks questions, else `waitingPermission`.
2. `status == waiting`:
   - `waitingFor == "permission prompt"` → `waitingPermission`.
   - `waitingFor` is anything other than `"dialog open"` → `waitingInput`.
   - `"dialog open"` → `waitingInput` **only if** an asking Notification arrived at or after `statusUpdated − 5 s`. Otherwise it is a dialog the person opened (/config, /usage), which waits on nobody, so fall through.
3. `working` = if a fact is newer than `statusUpdated` (or the status is unknown) then `turnOpen`, else `status == busy`. If working → `runningTool` when the turn is open and a tool is open, else `generating`.
4. Background:
   - status `shell` and (no running background jobs known, or any non-service job running) → `background`.
   - status `busy` (but not working per rule 3) and a non-service job **with a task id** is running (a background subagent), or a listed subagent is still working → `background`.
   - status `idle` or `shell` ends the subagents last seen working before that change, and the jobs they were; `idle` also ends every job begun before it.
5. `turnEnded` exists, `completed`, `now − turnEnded < 10 min`, and `acknowledged < turnEnded` → `finished`.
6. Otherwise → `idle`.

`phaseSince` is kept while the phase stays the same between snapshots and reset to `now` when it changes (`:199-201`).

Session output fields (`:259-295`): `currentTool` only in runningTool/waitingPermission; `timeline` = the last 12 tools; `cost`/`tokens` = sum of usage per key; `lastActive = max(turnEnded, lastFact, statusUpdated)`; `waitingFor` only while waiting; `services` = count of running service jobs (only in shell status or background phase); `backgroundLabel` = the label of the newest running non-service job (previews only).

### 3.4 Events (notices)

`ClaudeSessionEngine.swift:202-216`. Raised only on a phase **change** (never on a session's first snapshot):
- → `waitingPermission`: `needsPermission(id, tool)`.
- → `waitingInput`: `needsInput(id)`.
- → `finished`, when the previous phase was active or waiting **and** `turnEnded ≥ previous.since`: `finished(id, duration = turnEnded − turnStarted)`. Going back to a result already shown (after a recap) is not news.

Handling (`NotchService.swift:4181-4214`): if auto-expand is on (default true), the island is not expanded, and it is not suspended (recording, fullscreen), then needsPermission/needsInput **open the island on that session's page** and show no notice. Otherwise a notice is shown:
- Permission: title = session display name, detail = `"Needs permission · <permission.summary ?? tool>"`.
- Input: `"Waiting for you · <question summary>"`.
- Finished: `"Done after 4m 12s"`, or `"Done"`.

Sessions notices have priority 2 and last **6 s** (`NotchSupport.swift:1627, 1637`). Clicking a notice opens the Sessions page focused on that session (`NotchService.swift:2400`). The notice icon is the session's Clawd (18×18, no bubbles) when buddies are on (`NotchNoticeView.swift:99-103`, `NotchCapsuleViews.swift:157-165`).

### 3.5 "Asleep" (doze) and acknowledgement

- `acknowledge(id)` is called on: opening the page (`focus`), sending a reply successfully, deciding a permission, and Open (jump). It sets `acknowledged = now`, so `finished` turns into `idle` (`ClaudeSessionService.swift:198-202, 229-234, 256, 301, 432`).
- **Doze** (UI only, `NotchClaudeSessionsSupport.swift:186-212`): phase `idle` and `now − max(lastActive ?? phaseSince, lookedAt) ≥ 5 min` → the Clawd sleeps and the row fades to 0.65 opacity (not while hovered or keyboard-focused, not with increased contrast, and not with Clawds turned off).

### 3.6 Counts and ordering

- Counts (`ClaudeSessionModels.swift:275-295`): `working` = isActive (generating, runningTool, background); `waiting` = isWaiting; `finished`.
- Order (`NotchClaudeSessionsSupport.swift:36-69`): rank waitingPermission 0 < waitingInput 1 < working 2 < background 3 < finished 4 < idle 5. Within a rank:
  - waiting: oldest `phaseSince` first
  - working: oldest `turnStarted` first
  - background/finished: newest `phaseSince` first
  - idle: newest `lastActive` first

  Ties: project name (localized standard compare), then pid.

---

## 4. UI

General: island content is white text on black. Font is the system font (SF); use Segoe UI Variable on Windows. Sizes are in points (= DIPs on Windows). The section's strings are English-only for every language (`NotchClaudeSessionStrings.swift:136-141`). Every string is in `NotchClaudeSessionStrings.swift:143-247`; port that table as is.

### 4.1 Colors

| Token | Value | Ref |
|---|---|---|
| Claude tint (working, progress, send button, "SUMMARY" rule) | `#D97857` (0.85, 0.47, 0.34) | `NotchAgentComponents.swift:9-16` |
| background phase | Claude tint at 75% opacity | `NotchClaudeSessionsView.swift:296` |
| waiting (permission/input) | `#FFC747` | `:297` |
| finished, Allow button | `#5CD685` | `:298`, `NotchClaudeSessionDetailView.swift:609` |
| idle | white 45% | `:299` |
| Deny button | system red at 32% | `NotchClaudeSessionDetailView.swift:610` |
| secondary button | white 12% | `:611` |
| link | `#6BA8FF` | `NotchClaudeSessionsView.swift:306`, `NotchClaudeRichText.swift:20` |
| error text | system orange | reply field |
| project palette (8) | `#6BA3FF #B88CFF #FF80BD #45D1D6 #9ED65C #D6AD85 #61DBFF #DBC7FF` | `NotchClaudeSessionsView.swift:235-240` |

Project color index = FNV-1a 64 (offset `0xcbf29ce484222325`, prime `0x100000001b3`) over the UTF-8 of `project.lowercased()`, `% 8` (`NotchClaudeSessionsSupport.swift:137-144`). The project name is the last path component of `cwd`, with a `/.claude/worktrees/...` suffix cut so worktrees show their main project (`AgentLogParser.swift:238-243`).

Phase symbols (SF Symbols → use an icon font on Windows; `NotchClaudeSessionsSupport.swift:109-119`): permission `hand.raised.fill`, input `questionmark.bubble.fill`, runningTool `hammer.fill`, generating `sparkle`, background `hourglass`, finished `checkmark.circle.fill`, idle `moon.zzz.fill`.

### 4.2 Compact strip (closed island, live activity)

`NotchClaudeSessionStrip.swift`. Shown when `notchSessionsLiveActivity` (default true), the service is running, and the counts are non-zero (`NotchService.swift:509-512`). Sessions come before the AI-usage strip in activity priority (`NotchSupport.swift:1789-1798`).
- Left wing: the mark. Hand (waiting tint) if any session is waiting; the animated Claude glyph if any is working; else the check (finished tint). Size 14.
- Right wing: counts, in the order waiting (hand symbol), working (sparkle), finished (check). Zeros are omitted. Symbol size = 0.7 × text size, bold; number semibold, monospaced digits; each in its phase tint; 6 pt gap. The wing width is computed from `items·(size·0.9+2) + digits·size·0.62 + (items−1)·6` (`NotchClaudeSessionsSupport.swift:101-107`).
- Click: open the Sessions page. If any session is waiting, focus the first one (`NotchClaudeSessionStrip.swift:57-62`).

### 4.3 List page

`NotchClaudeSessionsView.swift`:
- Band (top-leading, beside the camera): counts with 9 pt symbols and 11 pt numbers, or the total in secondary when every count is zero. Height 28.
- Empty: "No sessions running" / "Start Claude Code in Terminal or Antigravity and it shows up here."
- Rows: `LazyVStack` with spacing 6, spring animation on reorder (response 0.35, damping 0.85).

**Row** (`:121-227`):
- Height 46. Corner radius 14 (continuous). Control-surface background is white 6.5%. Leading padding 11.
- If waiting: 1 pt border in the waiting tint at 55%.
- Avatar: Clawd 32×30, or a phase badge 26 when buddies are off. The badge is a circle at tint 16% with the symbol at 0.42 × size, or the working Claude glyph.
- Line 1: display name, 12 semibold, white 95%, one line, tail truncation. Right side: the clock (11 medium, monospaced, tint). The clock shows `m:ss` / `h:mm:ss` since the turn started (working) or since `phaseSince` (waiting, background), and nothing for finished/idle (`NotchClaudeSessionsView.swift:316-325`, `NotchAgentSupport.swift:396-400`).
- Line 2 (10.5 pt): project tag (semibold, project color on its color at 16%, capsule, padding 6/1.5, max width 160, middle truncation), then the activity text (waiting tint while waiting, else secondary), then `⎇ branch` (tertiary, middle truncation).
- Chevron 9 pt at white 30%, unless quick decision.
- **Quick decision**: when `permission.fitsOnALine`, show compact Deny (✕) and Allow (✓) buttons, 28×28 capsules, inline.
- Accessibility label: `"<name>, <project>, <phase>"`; value = the activity text.

**Display name** (`NotchClaudeSessionsSupport.swift:149-154`): the custom island name, else the registry `name` when `nameSource == "user"`, else `title` (ai-title, previews only), else the registry `name`, else the project.

**Activity text** (`NotchClaudeSessionsView.swift:343-374`), parts joined by `" · "`, empty parts dropped:
- First: the phase label.
- waitingPermission: + tool + (`detail` if it fits on a line, else `summary`).
- runningTool: + `"Tool: summary"` or the tool name.
- waitingInput: + the question summary, or the title (if the title is not already the name).
- generating: + title.
- background: + backgroundLabel, or title; + cost if > 0.
- finished/idle: + title; + cost if > 0.
- + `"N left running"` when services > 0 and not working.
- + `"done/total"` tasks while working.

### 4.4 Session page (detail, normal size)

`NotchClaudeSessionDetailView.swift`:
- Band: leading back button labeled with the display name. Trailing buttons (28×28, radius 9, 11 pt symbols): Rename (pencil, or a check while editing), Enlarge/Shrink, and Open (icon + "Open", or "Couldn’t find that window" for 3 s on failure; disabled when the host is unknown).
- Rename field: appears above, height 34, radius 10, white 8% background and 25% border, 13 semibold. The placeholder is the session's own name. An empty value restores the original. Names are trimmed and capped at 60 chars; at most 100 are kept, and names of sessions no longer running are pruned first (`NotchClaudeSessionsSupport.swift:157-180`).
- Scrolling content (spacing 10):
  1. **Header**: Clawd dress button 40×36 (badge 30) with 12 pt top padding (room for a hat jump). Phase label 12 semibold in tint; clock 11; when running a tool, `"Tool: summary"` 11 monospaced white 75%, middle truncation. Line 2: project tag + `"⎇ branch · <model display name> · <tokens e.g. 48K> · <cost e.g. $1.23>"` 10.5 secondary.
  2. **Permission card**, when waiting (§4.6).
  3. Title: 13 semibold, at most 2 lines (previews only).
  4. Tasks: `"%d of %d tasks"` 10.5 medium white 80% + the current task in secondary; a 4 pt capsule progress bar in the Claude tint on white 10%, minimum fill 4.
  5. **Recent**: section title (9 semibold, tracking 0.6, white 45%, uppercase). Horizontal scroll of tool chips: height 22, capsule, white 12% while running else 6%. Icon 9 pt: `circle.dotted` in the Claude tint (running), `xmark.circle.fill` red (failed), `checkmark.circle.fill` white 45%. Text `"Name summary"` 10.5 (monospaced when there is a summary), max width 180. Auto-scrolls to the newest.
  6. **Subagents**: at most 4 shown (running first by start, then done newest first; `ClaudeSessionEngine.swift:448-457`), then `"N more"`. Row: Clawd 18×14 (50% opacity when done) or a branch/check icon; type 11 medium; description 10.5 secondary (previews only); elapsed clock 10 right-aligned.
  7. **Last reply**: `"LAST REPLY"` + rich text 11 pt, white 80%, max 6 lines tail-truncated. With previews off: `"Turn on previews in Settings to see what Claude wrote."` 10 tertiary.
- Below the scroll: the thinking row (size 13) when working, then the **reply field** (§4.8).

### 4.5 Enlarged page

`NotchClaudeSessionLargeView.swift`. Toggled from the page band. It applies only while a session page is focused, and leaving the page shrinks the island again (`NotchService.swift:1531-1539, 4092-4096`).
- Island size: width `min(max(min(1180, screen.w·0.66), cameraWidth+36), screen.w − 24 − 2·gutter)`, height `min(screen.h·0.8, screen.h − 48 − bottomInset)` (`NotchSupport.swift:2662, 2734-2736`).
- Text size: preference `notchSessionsLargeTextSize`, default **15**, range **12…24**, rounded to whole points. Changed with the A−/A+ header buttons, ⌘+ / ⌘= / ⌘− (`:14, 91-99`, `NotchClaudeSessionsSupport.swift:290-294`). Let S = the size.
- Header: Clawd 48×42 (badge 34). Phase 15 semibold; clock 14; running tool 13 monospaced. Line 2: project tag at 12, then `branch · model · tokens · cost · cwd` 12 secondary, selectable. Buttons 30×30: smaller, larger, sidebar toggle (`sidebar.right`).
- Permission card: wrapped in a fitted scroll. Max height `cardLimit = max(120, min(pageH·0.6, pageH − headerH − composerH − 36 − 80))`, 360 before the page is measured. It scrolls back to the top when the session or request changes (`:102-109, 576-597`).
- **Conversation** (left, flexible):
  - Previews off → the previews-off message at S−1.
  - Empty and not working → "Nothing said yet."
  - Otherwise a lazy list with spacing S, anchored to the bottom; a new message scrolls to the end (animated 0.2 s).
  - User bubble: right-aligned, at least 60 pt from the leading edge, S pt text white 95%, padding 14/9, white 10% background, radius 14; time below (10 pt tertiary).
  - Assistant message that **ends a turn** ("answer"): a rule row (`"SUMMARY"` at max(10, S−4) semibold, tracking 0.8, Claude tint; time; a 1 pt line in the Claude tint at 35%), then rich text S at white 96%, line spacing S·0.25, top padding S·0.5.
  - Replies "on the way" (before an answer in a finished turn) at white 60%. Replies in the running turn at white 88%.
  - The thinking row at the end while working.

  Classification (`NotchClaudeSessionsSupport.swift:273-287`): walk the chat newest first. A user message resets; an assistant `endsTurn` is an answer and sets answered; any assistant after that (older) while answered is "on the way".
- **Sidebar** (right, 330 wide, black background, slides over the conversation): title 15 semibold; tasks (12 pt + system progress bar in tint); **Recent** tools newest first, folded to 4 rows with "Show all N"/"Show less" (11 medium in a capsule button, 24 high); tool row = icon 10 + name 12 semibold + summary 11 monospaced middle-truncated + duration 10. **Subagents**: same folding; avatar Clawd 22×17; type 12 semibold; description 11 (2 lines); clock 10.
- Sidebar handle column, 38 wide: a 1 pt line at white 8% (22% hovered), a chevron in an 18×34 capsule, `Color(white 0.12)` (0.2 hovered). Clicking folds the sidebar; the state persists in `notchSessionsSidebarHidden`. Fold animation: first resize the conversation with no animation and text dimmed to 25%, then slide the sidebar (smooth, 0.38 s) and fade the text back (0.3 s). With reduce motion, switch at once (`:111-132`).
- **Composer**: attachment strip with 54 pt thumbs. Multi-line editor, height S·4 (≈3 lines, then it scrolls), padding 10, white 7% background, radius 14, border white 8% (30% focused, 60% while dragging a drop). Send button: 38 circle, Claude tint when sendable (black arrow) else white 8%. Footer 11 pt: an error in orange on the left; `"⌘V or drop to add images · ⌘↩ sends"` tertiary on the right. Placeholder: "Reply to Claude…", or the unavailable reason.

### 4.6 Permission card

`NotchClaudeSessionDetailView.swift:317-406`:
- Container: padding 10, background tint 8%, 1 pt border tint 35%, radius 12.
- Title row: phase symbol 11 + title 12 semibold, in the tint. Title = `"Claude asks"` for questions, else `permission.tool ?? currentTool.name ?? waitingFor ?? phase label`.
- **Settling**: on every new requestID (the first one included), all buttons are disabled for 0.8 s.
- Tool request:
  - Detail: the whole command/path/arguments (§2.6), 11 pt monospaced, white 90%, selectable, in a scroll of max height 120, padding 8, black 35% background, radius 8. It is shown even with previews **off**, because nobody should allow what they cannot read.
  - Buttons (28 high, capsule; icon 10 bold + title 11.5 semibold, padding 11): `Deny` (✕), spacer, `Always` (`checkmark.seal`, only if `offersAlways`), `Allow` (✓, green with black text).
  - Hint: "⌘Y allows and ⌘N denies while the island has the keyboard." 9.5 tertiary.
- The registry says waiting but the hooks hold no permission (no hooks, or a dialog): `"Answer this one in the terminal."` + an `Open` button.
- Keyboard (`NotchService.swift:1886-1904`): ⌘Y = allow, ⌘N = deny (Command alone). Target: the focused session, else the first session holding a tool permission. Ignored while the wardrobe is open. On a question only ⌘N (skip) works.

### 4.7 Question form

`NotchClaudeSessionDetailView.swift:408-579`:
- Per question (spacing 12):
  - Optional header chip: uppercase, 9 semibold, tracking 0.6, white 55%, capsule at white 8%.
  - Question: 12 semibold, white 95%, selectable.
  - For multiSelect: "Pick any that apply" (10 secondary).
- Option rows (spacing 4): button min height 32, padding 9/7, radius 9. Background is tint 18% when selected, else white 6%; border tint 60% when selected. Marker 16×16: multi `square` / `checkmark.square.fill`, single `circle` / `largecircle.fill.circle`. Label 11.5 semibold; description 10.5 white 60%, max 3 lines.
- **Preview**: when the picked option has a `preview`, it is shown below in 10.5 monospaced, scroll max height 140, black 35%, radius 8.
- **Own answer**: a text field (height 28, radius 9, white 6%, 11.5 pt). Placeholder "Or write your own answer…", or "Reply to Claude…" when there are no options. For a single-select question, typing clears the pick, and picking clears the typed text. For multi-select, the typed text is appended to the picks. Return submits.
- **Answer on click**: exactly 1 question, single select, no option has a preview → clicking an option sends at once.
- Footer: `Skip` (deny, `forward` icon) | `Answer` (allow style, `arrow.up`). Answer is enabled only when every question has ≥ 1 answer. Then the hint: "Your answer goes straight to Claude. Return sends what you wrote, and ⌘N skips the question."
- After sending, the form disables for 2 s; if the request is still shown (it was answered in the terminal meanwhile), it re-enables.
- The answers map follows option order as Claude wrote it (`NotchClaudeSessionsSupport.swift:355-367`).

### 4.8 Reply field (normal page)

`NotchClaudeSessionDetailView.swift:616-701`:
- Attachment strip with 36 pt thumbs. A one-line field, 12 pt, height 32, white 7% background, radius 10, border white 30% focused / 60% dragging. Send button: 24 circle (Claude tint when sendable). Return or ⌘↩ sends.
- Unavailable reasons, which also disable the field:
  - waiting → "Answer the prompt first"
  - `waitingFor` set (a dialog) → "Close the dialog in the terminal first"
  - the host does not accept replies → "Replies work in Terminal and Antigravity"
- Errors under the field (10 pt orange with ⚠): `.waiting` → "It’s waiting on a prompt now. Answer it first."; `.unreachable` → "Couldn’t reach that terminal".
- The draft is kept per session in memory while the island closes. It is cleared on success.
- Reply text = trimmed, max **4000** chars (`NotchClaudeSessionsSupport.swift:130, 385-389`).

### 4.9 Image attachments

`ClaudeImageAttachments.swift`. Claude Code attaches an image when a paste is nothing but the path of a PNG/JPEG/GIF/WebP file.
- Input: ⌘V (if the clipboard holds image files, or image data with no text) or drag-and-drop of files/image data onto the field or composer. Otherwise ⌘V pastes text.
- Limits: at most **5** per reply, each ≤ **20 MiB**. Duplicates are ignored.
- png/jpg/jpeg/gif/webp files are used where they lie. Any other image, or raw image data, is converted to PNG in `$TMPDIR/vorssaint-images/<UUID>.png` (folder 0700, file 0600). Files there older than 24 h are swept at start.
- Thumbnails: a radius-8 square with a white 15% border and an ✕ remove button (14 pt) offset (5, −5).
- For the fallback "naming" mode (an old extension), the images are copied to `/private/tmp/claude-<uid>/<project-folder-of-transcript>/<sessionId>/images/vorssaint-<UUID>.<ext>` if that folder exists. Claude reads there without asking. Otherwise the original paths are used.

### 4.10 Jump-to-window ("Open")

Covered in §6.1. In the UI: the Open button and the card's Open button. A failure shows "Couldn’t find that window" for 3 s.

### 4.11 Thinking row

`NotchClaudeSessionLargeView.swift:432-480`, `NotchClaudeSessionsSupport.swift:250-267`:
- Glyph: `· ✢ ✳ ✶ ✻ ✽`, played forward and back (cycle 10), at 8 steps/s, in the Claude tint at S+1 semibold. Reduce motion → a static `✻`.
- Word: one of 16, picked per turn: `["Thinking","Pondering","Brewing","Musing","Noodling","Percolating","Simmering","Tinkering","Mulling","Crafting","Conjuring","Pontificating","Ruminating","Churning","Whirring","Cogitating"][floor(turnStarted seconds since **2001-01-01T00:00:00Z**) mod 16]`, followed by `…`. For parity use `unix − 978307200`.
- Shimmer: a band 3 letters wide moves at 14 letters/s over a span of `len + 12`. Each letter's glow is `max(0, 1 − |i − center|/3)`, blending the tint toward white by `glow·0.7`. The view refreshes at ≤ 24 fps.
- After the word: the clock (S−3, tertiary) and the tool name when running.

### 4.12 Cost, tokens, tasks, subagents

- Tokens `total = input + cacheWrite + cacheRead + output`, formatted `n`, `4.2K`, `48K`, `1.2M` (one decimal below 10, floored). Cost: `$0.42`, `$123`, `$12K` (`NotchAgentSupport.swift:337-363`). Model display name comes from `AgentPricing.displayName` (see 03-agents.md).
- Cost and tokens accumulate across the whole transcript of the session (including its subagents' assistant lines, because those are read through the same reader per file; each subagent file has its own reader, but the usage keys are global message ids).

---

## 5. Clawd buddies

### 5.1 Data model

`ClawdCostume.swift`:

| Part | Cases, in **this exact order** (the order affects seeding) |
|---|---|
| Shell | `clay, coral, amber, moss, mint, teal, sky, iris, orchid, rose` |
| Hat | `none, beanie, cap, beret, topHat, crown, party, propeller, chef` |
| Eyewear | `none, glasses, shades, round, monocle, anaglyph` |
| Extra | `none, bow, scarf, bowTie, flower, headphones, coffee` |

Shell RGB (sRGB 0-255): clay 215,119,87 (Claude Code's own `clawd_body`); coral 226,86,72; amber 230,166,58; moss 112,168,70; mint 72,184,146; teal 40,160,170; sky 68,146,224; iris 116,112,226; orchid 178,98,214; rose 230,112,156.

`classic` = clay/none/none/none.

Persisted code: `"v1:<shell>.<hat>.<eyewear>.<extra>"` using the raw names above (e.g. `v1:teal.none.monocle.none`). When reading, the code must start with `v1:`, be ≤ 80 chars and have exactly 4 parts. An unknown part falls back to the seeded costume's part; anything else is invalid → the seeded costume (`ClawdCostume.swift:147-179`).

### 5.2 Seeding (must be bit-exact across platforms)

`ClawdCostume.swift:85-143, 181-195`:

```
seed(key)    = FNV-1a 64 over UTF-8 bytes of key: h = 0xcbf29ce484222325; for b: h ^= b; h = h * 0x100000001b3 (mod 2^64)
generator    = SplitMix64(state = seed):
               next(): state += 0x9e3779b97f4a7c15; z = state;
                       z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9;
                       z = (z ^ (z >> 27)) * 0x94d049bb133111eb;
                       return z ^ (z >> 31)          (all mod 2^64)
draw:
  1. shell: weights clay=3, others=1 (total 12); ticket = next() % 12; walk the shells in order subtracting weights.
  2. hat     = pick(hats,    chance 0.70)
  3. eyewear = pick(eyewear, chance 0.40)
  4. extra   = pick(extras,  chance 0.45)
pick(parts, chance):            ← ALWAYS consumes two next() calls, in this order
  roll  = (next() >> 11) / 2^53          (double in [0,1))
  index = next() % (len(parts) - 1)
  return roll < chance ? parts[1 + index] : parts[0]
```

Seven `next()` calls in total per costume. The key is the **session id** for a session, and `"<sessionId>/<agentId>"` for a subagent (`NotchClaudeSessionsSupport.swift:237-239`). A subagent whose definition has a `color:` gets that shell: red→coral, orange→clay, yellow→amber, green→moss, cyan→teal, blue→sky, purple→iris, pink→rose; anything else keeps the drawn shell (`ClawdCostume.swift:36-48, 104-108`). Subagents cannot be dressed by hand.

Test vectors (pinned by `ClawdCostumeTests.swift:15-34`, and re-derived independently with a Python port for this spec):

| Input | Output |
|---|---|
| `seed("")` | `0xcbf29ce484222325` |
| `seed("a")` | `0xaf63dc4c8601ec8c` |
| SplitMix64(0) first two | `0xe220a8397b1dcdaf`, `0x6e789e6aa1b965f4` |
| `"99c35831-5f0b-4c3e-9d0a-0f3c2a7b8e11"` | `v1:teal.none.monocle.none` |
| `"0f9a6c1e-2b7d-4e55-8a31-c4d2e6f7a890"` | `v1:orchid.beret.anaglyph.coffee` |
| `"s/a1"` | `v1:clay.propeller.none.scarf` |

**[WIN]** In TypeScript use `BigInt` with `& 0xFFFFFFFFFFFFFFFFn`, or do it in Rust and pass costumes to the UI. `roll` must be computed as `Number(next >> 11n) / 2**53`.

"Surprise me" (`:111-117`) uses the system RNG and redraws up to 10 times while the result equals the current costume.

`kept(costume, key)` stores nothing when the chosen costume equals the seeded one, so the choice follows the lot.

### 5.3 Persistence

- `notchSessionsCostumes`: `{sessionId: code}`. `notchSessionsNames`: `{sessionId: name}`. Each keeps at most 100 entries; when over the limit, sessions that are not live are removed first, in sorted-key order (`NotchClaudeSessionsSupport.swift:170-180`).
- The Mac keeps both out of settings backups because session ids are per machine (`Defaults.swift:897-905`).

### 5.4 Sprite format

`NotchClawdSprite.swift`. Everything is plain data, so port it as data.
- Canvas **24×22** px. The body's top-left is at canvas (5, 12). The body is 13×8 of `C`.
- Fitting frame: columns 3..<20 (17), rows 8..<22 (14). This frame is the same for every costume, so all Clawds draw at the same scale; hats, raised arms and bubbles may overflow the box (no clipping; `masksToBounds = false`).
- Integer scaling: `devicePx = max(1, floor(min(boxW·scale/17, boxH·scale/14)))`; pixel = devicePx/scale. The frame is centered and the origin snapped to device pixels. Nearest-neighbour, no smoothing (`:89-101`).
- Composition order: legs → body → arms → eyes (two, at body columns 3 and 9) → eyewear → hat → extra → bubble. Stamps use body coordinates; `.` is transparent. The body, arms, eyes and costume rise by `rise + jump`; legs rise only by `jump`.
- Legs at body x = 0, 2 (left) and 10, 12 (right). Each spans rows `8 − rise` … (8 if jumping or lifted, else 9), letter `c`. `stepLeft` lifts the left legs and `stepRight` the right.

Full stamp tables, verbatim (x, y relative to the body's top-left; rows top to bottom):

```
ARMS
 downLeft  (-2,4) ["CC","CC"]            downRight (13,4) ["CC","CC"]
 upLeft    (-3,0) ["CC.","CC.",".CC",".CC","..C"]
 upRight   (13,0) [".CC",".CC","CC.","CC.","C.."]
 typeLeft  = (-2,5)["CC","CC"] + downRight      typeRight = downLeft + (13,5)["CC","CC"]
 waveUp    = downLeft + upRight                  waveOut  = downLeft + (13,2)["..C",".CC","CC.","C.."]
EYES (x = 3 or 9)
 open (x,2)["E","E"]   closed (x-1,3)["EEE"]   happy (x-1,2)[".E.","E.E"]
 left (x-1,2)["E","E"] right (x+1,2)["E","E"]
EYEWEAR
 glasses  (0,1) [".KKKKK.KKKKK.","KK...KKK...KK",".K...K.K...K.",".KKKKK.KKKKK."]
 shades   (0,1) ["KKKKKKKKKKKKK",".LlLLL.LlLLL.",".LLLLL.LLLLL.","..LLL...LLL.."]
 round    (1,1) [".KKK...KKK.","K...KKK...K","K...K.K...K",".KKK...KKK."]
 monocle  (7,1) [".YYY.","Y...Y","Y...Y",".YYY.","....y","...y.","...y."]
 anaglyph (0,1) ["WWWWWWWWWWWWW","WRRRRRWTTTTTW","WRRRRRWTTTTTW",".WWWWW.WWWWW."]
HATS
 beanie (0,-4) [".....WWW.....","...AAAAAAA...",".AAAAAAAAAAA.","AAAAAAAAAAAAA","aAaAaAaAaAaAa"]
 cap    (0,-3) ["...AAAAAAA......",".AAAAAWAAAAA....","AAAAAAAAAAAAaaaa"]
 beret  (-1,-4)[".........a....","...AAAAAAAAA..",".AAAAAAAAAAAAA","AAAAAAAAAAAAAA",".aaaaaaaaaaa.."]
 topHat (1,-6) ["..DDDDDDD..","..DdDDDDD..","..DdDDDDD..","..RRRRRRR..","..DDDDDDD..","DDDDDDDDDDD"]
 crown  (1,-4) ["Y....Y....Y","YY..YYY..YY","YYYYYRYYYYY","yyyyyyyyyyy"]
 party  (3,-7) ["...Y...","...A...","..WWW..","..AAA..",".WWWWW.",".AAAAA.","WWWWWWW"]
 propeller (1,-5) [tick==0 ? "RRRR.s.BBBB" : "..RR.s.BB..", ".....s.....","...RRYBB...",".RRRRYBBBB.","RRRRRYBBBBB"]
 chef   (1,-6) ["..WWW.WWW..",".WWWWWWWWW.","WWWWWWWWWWW",".WWWWWWWWW.","..WWWWWWW..","..wwwwwww.."]
EXTRAS
 bow        (9,-2) ["AA.AA","AAaAA","AA.AA"]
 scarf      (0,4)  ["AAAAAAAAAAAAA","aaaaaaaaaaaaa",".........AA..",".........aA.."]
 bowTie     (4,4)  ["KK.KK","KKsKK","KK.KK"]
 flower     (-1,-2)[".W.","WYW",".W."]
 headphones (-2,-3)["....SSSSSSSSS....","..SS.........SS..",".S.............S.",".S.............S.",
                    "AA.............AA","AA.............AA","AA.............AA"]
 coffee     (hand.x, hand.y-2) steam(tick==0 ? [".w.","w.."] : ["w..",".w."]) + ["NNN","WWW","WWW"]
            right hand: down/typeLeft (15,3); typeRight (15,4); up/waveUp (15,-2); waveOut (16,0)
BUBBLES
 alert    (15,-7) ["XX","XX","XX","..","XX"]
 question (15,-7) ["XXX","..X",".X.","...",".X."]
 sleep(p) small=(13,-3)["ZZZZ","..Z.",".Z..","ZZZZ"]  large=(14,-9)["ZZZZZ","...Z.","..Z..",".Z...","ZZZZZ"]
          p%3==0 small; ==1 small+large; ==2 large
```

Palette (`:447-495`):
- `C` = shell; `c` = shell × 0.78 (integer `*78/100`); `A` = accent shell; `a` = accent × 0.78.
- Accent per shell: clay→sky, coral→teal, amber→iris, moss→coral, mint→orchid, teal→amber, sky→coral, iris→amber, orchid→mint, rose→sky.
- Fixed colors: E 24,22,26 · K 32,32,38 · D 66,66,78 · d 108,108,124 · L 30,30,40 · l 150,170,205 · W 246,244,238 · w 196,200,210 · R 226,64,64 · Y 247,198,70 · y 196,142,44 · T 64,204,232 · N 124,76,44 · B 72,132,232 · S 156,158,170 · s 98,100,112 · X 255,199,71 · Z 196,206,236.
- Every pixel is either opaque or clear.
- Cache images by `costume.code + "|" + pose.key` (512 entries). Pose key = `arms.legs.eyes.bubble.rise.jump.tick`.

**[WIN]** Recommended approach: generate RGBA 24×22 in TS from these tables and draw on a `<canvas>` with `imageSmoothingEnabled = false` and integer scaling via `devicePixelRatio`. Export the tables as JSON (`clawd-sprite.json`) shared by Rust and TS.

### 5.5 Motions and animation

Motion per session phase (`NotchClaudeSessionsSupport.swift:189-199`): generating→`think`, runningTool→`work`, background→`slow`, waitingPermission→`alert`, waitingInput→`ask`, finished→`content`, idle→`doze` if dozing else `rest`. Subagents: running→`work`, done→`done` (`:232-234`). Notices: the finished symbol→`content`, the input symbol→`ask`, otherwise `alert`, with no bubbles (`:223-229`).

Pose function (`NotchClawdSprite.swift:113-218`). Rate = 12 steps/s (halved in Low Power Mode **[WIN]**: Energy saver). `t` = seconds since the motion began, where the start is back-dated to `session.phaseSince` so every Clawd of one session moves as one. `step = floor(t·12)`; `tick = (step/3) % 2`.
- `rest`: eyes = blink ? closed : glance; rise = breath(2.4 s).
- `content`: happy eyes; breath 2.4.
- `doze`: closed; breath 3.2; bubble `sleep(floor(t/0.7) % 3)`.
- `think`: arms alternate typeLeft/typeRight every 2 steps; eyes cycle every 0.8 s: [open, left, open, right]; blink overrides.
- `slow`: arms alternate every 4 steps; blink.
- `work`: `(step/2) % 4` → 0 stepLeft, 2 stepRight, 1 and 3 rise = 1; blink.
- `alert` / `ask`: bubble `!` / `?`. Cycle 1.6 s. For cycle < 0.5 s, jump = `[0,2,3,3,2,0][min(5, floor(cycle·12))]` with arms up while jumping. For 0.5–1.1 s the arms wave, alternating waveUp/waveOut every 0.15 s. Eyes open.
- `done`: happy, tick 0.
- `blink(t, seed)`: period = `3.4 + (seed % 23)/10`, offset = `((seed >> 8) % 1000)/1000 · period`; closed while `(t + offset) mod period < 2/12`.
- `glance`: period = `6.1 + ((seed >> 16) % 31)/10`, offset = `((seed >> 24) % 1000)/1000 · period`; for 0.8 s look left on even periods, right on odd.
- `breath(p)`: rise 0 for the first half of the period, 1 for the second.
- **Cheer** (a hop when work finishes, or after a wardrobe change): it lasts 1.4 s from `phaseSince` of the finished phase, or from the moment the costume changed. Pose: happy eyes; `step < 9` → jump `[0,0,2,4,5,5,4,2,0][step]`, arms down for step < 2 then up; afterwards arms up.
- **Reduce motion** gives still poses: rest default; content/done happy; doze closed + `sleep(1)`; think/slow typeLeft; work stepLeft; alert `!` + waveUp; ask `?` + waveUp.
- The animation clock runs only while the view is visible (window visible, not occluded, not hidden). The layer is updated only when the pose changes (`NotchClawdView.swift:135-170`).
- `seed` for rhythm = `ClawdCostume.seed(session.id)` (or the subagent key).

### 5.6 Where Clawds appear, with sizes

List row 32×30 · page header button 40×36 (badge 30) · large header 48×42 (badge 34) · subagent row 18×14 / large 22×17 · notice 18×18 · settings samples 30×28 (classic; sky+cap+bow; moss+topHat+shades) · wardrobe tiles 34×30 · wardrobe stage (see below). With buddies off, every one of these becomes the phase badge (or the fallback icon for subagents).

### 5.7 Wardrobe UI

`NotchClawdWardrobeView.swift`. Opened by clicking the Clawd on a session page; a `tshirt.fill` badge (15 circle, white 92%) appears on hover with a spring.
- Band: back button "Wardrobe" (→ page). Trailing: "Surprise me" (dice icon) and Reset (`arrow.uturn.backward`, disabled unless dressed by hand; tooltip "Back to the original outfit").
- Waiting bar on top if the session starts waiting meanwhile: a capsule in the tint at 14%, height 30, "Needs permission … Go to it ›". It returns to the page.
- Stage (left): width `min(230, max(150, island.w·0.3))`, height = width·0.92, control surface radius 18. Pixel = `min((w−20)/24, (h−20)/22)`. The figure is sized to the fitting frame; the floor is an ellipse at white 7%. The live Clawd shows the session's motion, and hops on each change. Below: name 13 semibold + project tag.
- Right (scroll): "Shell" color dots (20 circle in a 28 hit area, 2 pt white ring when selected, adaptive grid 26–30), then "Hat", "Eyewear", "Extras" tile grids (adaptive 46–54, tile height 44, radius 10, white 13% selected / 5%, border white 85% 1.5 pt / 7% 1 pt). Each tile shows the session's shell wearing **only** that piece; the "none" tile shows a `line.diagonal` glyph. The headings show the chosen part's name on the right.
- Escape layering (`NotchClaudeSessionsSupport.swift:243-248`): wardrobe → page → list → close the island.
- Turning buddies off closes the wardrobe.

---

## 6. Reply delivery and jump-to-window

### 6.1 macOS (as built)

**Reply gate** (`ClaudeSessionService.swift:265-305`):
1. `canReply` = host accepts replies AND phase not waiting AND `waitingFor == nil`, and the text (or images) are non-empty. Otherwise `.waiting` (if waiting or in a dialog) or `.unreachable`.
2. On the queue: `asking` = the engine holds a permission OR a hook for this session is pending. Such a dialog may already be open in the terminal even if the registry does not say so yet.
3. On the actions queue: **re-read the registry** for the pid. A different sessionId → `.unreachable`; asking or status waiting → `.waiting`.
4. Deliver. On success, clear the draft and attachments and acknowledge.

**Terminal.app** (`ClaudeSessionHosts.swift:76-172`). The Automation consent check (`AEDeterminePermissionToAutomateTarget`) runs first; without consent the reply fails and Open just activates Terminal.
- Focus: iterate windows/tabs, match `tty of theTab` to the session's tty, `set selected`, `set index of window to 1`, `activate`. The script returns `"found"`/`"missing"`.
- Type: `do script (ESC & "[200~" & text & ESC & "[201~") in theTab`. Bracketed paste keeps newlines as text; `do script` appends the Return that submits.
- With images: one `do script` with each image path as its own bracketed paste followed by an **opening** `ESC[200~` (the Return Terminal appends is absorbed as a line break inside the open paste), then `delay 1.2 + 0.3·n` while Claude Code reads the images, then `do script (text & ESC[201~)` whose Return submits.

**Antigravity** (`ClaudeSessionService.swift:605-623`, `ClaudeSessionHosts.swift:174-271`, `Tools/AntigravityBridge/extension/extension.js`):
- Shell pid = **parent pid** of the claude pid. The VS Code API knows a terminal by its shell's pid (`terminal.processId`).
- Extension: activates `onStartupFinished` with `extensionKind: ["ui"]`. Each window's extension host listens on `~/.vorssaint/terminals/<extHostPid>.sock` (folder 0700, socket 0600, umask 077, `allowHalfOpen`, 5 s connection timeout, max request 64 KiB).
- Protocol: one request line of JSON + `\n`; one reply line of JSON + `\n`, after which the connection is ended.

  | Request | Reply |
  |---|---|
  | `{"command":"ping"}` | `{"ok":true,"version":"1.0.3"}` |
  | `{"command":"focus","shellPid":N}` | `terminal.show(false)` → `{"ok":true}` |
  | `{"command":"send","shellPid":N,"text":"…"}` | `sendText(ESC[200~ + text + ESC[201~, false)`, wait 80 ms, `sendText("\r", false)` → `{"ok":true}` |
  | `{"command":"send-images","shellPid":N,"text":"…","images":["/p1.png",…]}` | each image pasted, 60 ms apart; wait `1200 + 300·n` ms; paste the text if non-empty; wait 80 ms; `\r` → `{"ok":true}` |
  | `{"command":"steps","shellPid":N,"steps":[{"paste":"…"},{"text":"…"},{"enter":true},{"wait":ms≤10000}]}` | generic sequence → `{"ok":true}` (not used by the app yet) |
  | wrong window | `{"ok":false,"error":"missing"}` |
  | unknown or invalid command | `{"ok":false,"error":"unknown"}` |
  | exception | `{"ok":false,"error":"<message>"}` |
- Client: tries **every** `*.sock` in the folder in turn, stopping at the first `ok:true`. Timeout 4 s per socket (send-images: `4 + 1.2 + 0.3·n`). If no window has `send-images` (an old extension says `unknown`), it falls back to `send` with the text plus `\n\n[Image: <path>]` lines.
- Focus: read `<home>/ide/<CLAUDE_CODE_SSE_PORT>.lock` → `workspaceFolders`. `openFolder` = the deepest workspace folder that equals or prefixes the cwd. Activate the app first (otherwise Antigravity only reorders its own windows). If no open folder is known, raise the window whose AX title contains `windowHint` (the folder's last component) via the Accessibility API. If a folder is known, run `<Antigravity.app>/Contents/Resources/app/bin/antigravity-ide <folder>`, which brings the window holding that folder forward and never opens a new one, because only open folders are asked for. Then send the extension `focus`.
- Install: zip `Resources/AntigravityBridge` into a `.vsix`, then run `antigravity-ide --install-extension <vsix> --force`. "Installed" = a folder `~/.antigravity-ide/extensions/vorssaint.vorssaint-terminal-bridge-<version>` exists. "Outdated" = no installed version equals (or starts with) the bundled `package.json` version. An outdated extension is reinstalled automatically at start. Settings row: "Antigravity extension" with an Install/Update button.

**Other apps** (`.app(bundle)`): Open only activates the app; replies are unavailable. **Unknown**: Open is disabled.

### 6.2 Windows: proposed design

Goals: the same UX for (a) Windows Terminal, (b) VS Code / Cursor / Antigravity / Windsurf integrated terminals, and (c) legacy conhost windows. No WSL.

#### 6.2.1 Identify the host: claude pid → process tree **[VERIFY]**

1. Get the pid from the registry (`%USERPROFILE%\.claude\sessions\<pid>.json`) **[VERIFY: the registry exists on Windows, and its `pidDomain` value; see Open questions]**.
2. Walk the ancestors with `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` → `th32ParentProcessID`. Guard against pid reuse: a parent's creation time (`GetProcessTimes`) must be ≤ the child's.
3. Classify by the first ancestor whose image name matches:
   - `WindowsTerminal.exe` → `windowsTerminal { wtPid, shellPid }`
   - `Code.exe`, `Code - Insiders.exe`, `Cursor.exe`, `Antigravity.exe`, `Windsurf.exe` → `vscodeFamily { ideKind, idePid, shellPid, ssePort? }` **[VERIFY: the Antigravity exe name on Windows]**
   - `conhost.exe` / no terminal ancestor but the process has a console → `conhost`
   - `explorer.exe` reached → `unknown`
4. `shellPid` = the ancestor directly under the terminal/IDE process (pwsh.exe, powershell.exe, cmd.exe, bash.exe from Git Bash, nu.exe…). Keep the **whole ancestor chain** too: with the npm install, `claude` is `claude.cmd → node.exe`, so the direct parent is a `cmd.exe` shim, not the user's shell.
5. Optional environment hints, read from the PEB (`NtQueryInformationProcess(ProcessBasicInformation)` + `ReadProcessMemory` of `RTL_USER_PROCESS_PARAMETERS.Environment`; same user, `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ`): `WT_SESSION` (Windows Terminal tab GUID), `TERM_PROGRAM=vscode`, `CLAUDE_CODE_SSE_PORT`, `VSCODE_IPC_HOOK*` **[VERIFY: these are set on Windows; the WOW64/ARM64 PEB layout]**. Read only those keys and never store the rest.
6. Cache per pid, like the Mac does.

#### 6.2.2 Deliver text: three candidates, in order of preference

**A. VS Code-family companion extension (preferred for IDEs).** Port `extension.js` almost unchanged:
- Transport: a Windows named pipe `\\.\pipe\vorssaint-terminals-<userSidHash>-<extHostPid>` (Node `net.createServer().listen(pipePath)` supports pipes). Unix modes do not apply; rely on the default pipe DACL being the creating user + SYSTEM + Administrators, or create the pipe from a native helper with an explicit DACL **[VERIFY]**.
- Discovery: Windows has no pipe directory to list cheaply (`FindFirstFileW("\\\\.\\pipe\\*")` works but lists every pipe). Instead each extension writes a registration file `%LOCALAPPDATA%\Vorssaint\terminals\<extHostPid>.json` = `{"pipe":"\\\\.\\pipe\\…","version":"1.1.0","ide":"vscode|cursor|antigravity|windsurf","pid":<extHostPid>}`, removed in `deactivate`. The app ignores files whose pid is dead.
- Same JSON line protocol. Add `shellPids: [..]` (the ancestor chain) so the extension matches any `terminal.processId` in the chain. Keep `shellPid` for compatibility.
- Install: `code --install-extension <vsix> --force` (likewise `cursor`, `antigravity`, `windsurf` CLIs; find them via the IDE's install dir `bin\<cli>.cmd`). Detect installed versions under `~\<dataFolderName>\extensions`, with `dataFolderName` read from the editor's `product.json` (since VS Code 1.144 at `<install>\<build>\resources\app\`), from the editor's own `extensions.json` there (a removed extension's folder can outlive its entry, and isn't always in `.obsolete`), or from the folders when that file can't be read. Built in `hosts/bridge_installer.rs`. Verified end to end on VS Code 1.144 (`%LOCALAPPDATA%\Programs\Microsoft VS Code\bin\code.cmd`, `.vscode`); an open window loads a new install without a reload. **[VERIFY: the Cursor, Antigravity and Windsurf install folders and CLI names]**.
- Bracketed paste through `terminal.sendText` → ConPTY → claude.exe: **[VERIFY]**. ConPTY must pass `ESC[200~`/`ESC[201~` through to an app that enabled bracketed paste mode (`ESC[?2004h`), and Claude Code on Windows must treat it as a paste (newlines kept, no submit). If that fails, fall back to sending multi-line text with Claude Code's newline keystroke (`\` + Enter, or Shift+Enter **[VERIFY]**) per line, then `\r`.
- Also bump the version and **fix the `.vsixmanifest` version mismatch** (manifest says 1.0.1, `package.json` says 1.0.3).

**B. Windows Terminal and conhost: `AttachConsole` + `WriteConsoleInputW` (preferred for terminals).**
- From a small helper process (`vorssaint-console.exe`; a GUI app should not attach to consoles itself): `FreeConsole()`, then `AttachConsole(claudePid)` (that attaches to the pseudoconsole/OpenConsole that hosts the WT tab, or to the conhost window). Open `CONIN$` (`CreateFileW("CONIN$", GENERIC_READ|GENERIC_WRITE, FILE_SHARE_READ|FILE_SHARE_WRITE, …)`), build `KEY_EVENT` records (key down + key up per UTF-16 unit, `uChar.UnicodeChar` set, `wVirtualKeyCode = 0` or `VK_PACKET`), `WriteConsoleInputW`, `FreeConsole()`.
- Sequence: `ESC [ 2 0 0 ~` + text + `ESC [ 2 0 1 ~`, pause about 80 ms, then a `VK_RETURN` key event (`\r`).
- **[VERIFY]** Claude Code reads input in VT mode (`ENABLE_VIRTUAL_TERMINAL_INPUT`). Check that injected ESC sequences arrive as a bracketed paste and not as a literal Escape key, which would cancel. Fallback: per-line text with Shift+Enter (`KEY_EVENT` VK_RETURN with `SHIFT_PRESSED`) for newlines **[VERIFY: Claude Code honors Shift+Enter on Windows]**.
- **[VERIFY]** `AttachConsole` across integrity levels: an elevated claude cannot be attached from a non-elevated app. Report `.unreachable`.
- Images: write each path as its own paste, wait `1.2 + 0.3·n` s, then the text paste + Return. This mirrors the extension **[VERIFY: on Windows, does Claude Code attach an image when the paste is a bare `C:\…\x.png` path?]**.
- Advantages: no focus stealing, works while the window is in the background, does not need WT internals.

**C. `SendInput` (last resort).** Bring the window forward (§6.2.3), then type with `KEYEVENTF_UNICODE`, newlines as Shift+Enter, then Enter. It is fragile: focus races, an IME interfering, a user typing at the same time. Use it only when B fails (for example an elevated target, where it fails as well because of UIPI) and only after an explicit user action. It must never be the default.

**Hard rule kept from the Mac:** never type while the session is waiting (registry `status == "waiting"`, a held permission, or a pending hook). Re-read the registry just before sending (§6.1 steps 2-3).

#### 6.2.3 Jump to window ("Open") **[VERIFY]**

- **Windows Terminal**: since WT 1.18 a single `WindowsTerminal.exe` hosts all its windows, so the pid alone does not give the window.
  1. Preferred: after `AttachConsole(claudePid)`, `GetConsoleWindow()` returns the ConPTY pseudo-window; `GetAncestor(hwnd, GA_ROOTOWNER)` should be the hosting WT window **[VERIFY on WT ≥ 1.18]**.
  2. Fallback: enumerate the top-level windows of the WT pid (class `CASCADIA_HOSTING_WINDOW_CLASS`) and use UI Automation to find a `TabItem` whose name contains the session's terminal title. Claude Code sets the terminal title to something with the session name or ✳ **[VERIFY]**. Then `SelectionItemPattern.Select()`.
  3. Raise with `ShowWindow(SW_RESTORE)` + `SetForegroundWindow`. Foreground-lock rules: the app gets permission when it is handling the user's click. If needed, use the `AllowSetForegroundWindow` / `AttachThreadInput` technique.
- **VS Code family**: `code --reuse-window <openFolder>` (folder from `%USERPROFILE%\.claude\ide\<port>.lock` `workspaceFolders`, deepest prefix, **case-insensitive with normalized separators and drive letter**), then the extension's `focus`. Alternatively match the top-level window title (`… - <folderName> - Visual Studio Code`) and raise it. The lock file's `pid` field may give the IDE pid directly **[VERIFY: what that pid is on Windows]**.
- **conhost**: `GetConsoleWindow()` after attaching gives the HWND; raise it.
- Unknown: disable Open.

#### 6.2.4 Hook transport on Windows **[VERIFY: all of it]**

- No `nc`. Ship `vorssaint-hook.exe` (Rust, console subsystem, ~200 KB). It reads all of stdin, connects to `\\.\pipe\vorssaint-claude-hooks-<bundleId>-<userSidHash>`, writes the payload, signals the end of input (named pipes have no half-close, so frame it: send a 4-byte little-endian length + the payload, or rely on the existing "complete JSON" detection), reads the reply until the server closes, prints it to stdout, and always exits 0. If the pipe is absent it exits 0 at once and prints nothing. Connect timeout is about 200 ms.
- Server: the Rust app, `CreateNamedPipeW` with `PIPE_REJECT_REMOTE_CLIENTS`, a DACL limited to the current user SID, and `FILE_FLAG_FIRST_PIPE_INSTANCE` on the first instance. Get the client pid with `GetNamedPipeClientProcessId` for the "peer gone" check (`OpenProcess` + `WaitForSingleObject(h, 0)`), and verify the client's token user SID equals ours.
- The settings command must work whatever shell Claude Code uses to run hooks on Windows (Git Bash, cmd or PowerShell) **[VERIFY: which one, in Claude Code 2.1.3xx native Windows]**. Do not rely on `#` comments, `;`, `2>/dev/null` or quoting. Proposal: `"C:/Users/<u>/AppData/Local/Programs/Vorssaint/vorssaint-hook.exe" --app com.vorssaint.windows --marker vorssaint-hook` (forward slashes, double quotes). The marker becomes an **argument**, and the installer matches `command.contains("--marker vorssaint-hook") && command.contains("--app <bundleId>")`. If PowerShell is the hook shell, a quoted path needs `& `; test this. An alternative is to install into a path without spaces.
- Timeouts unchanged: 5 s, and 600 s for PermissionRequest. The app lets go at 590 s.

---

## 7. Preferences and privacy

| Key (macOS UserDefaults) | Default | Meaning | Ref |
|---|---|---|---|
| `notchSessionsEnabled` | **false** | section on | `Defaults.swift:891, 1498` |
| `notchSessionsLiveActivity` | true | strip beside the camera ("Show sessions beside the camera") | `:892, 1499` |
| `notchSessionsAutoExpand` | true | "Open the island when a session needs you" (never replaces an open page) | `:894, 1500` |
| `notchSessionsContentPreview` | **false** | "Show previews": titles, commands, file names, conversation (memory only; purged when turned off) | `:896, 1501` |
| `notchSessionsBuddies` | true | "A Clawd for every session" | `:900, 1504` |
| `notchSessionsLargeTextSize` | 15.0 | enlarged text size, 12…24 | `:908, 1502` |
| `notchSessionsSidebarHidden` | false | enlarged sidebar folded | `:906, 1503` |
| `notchSessionsNames` | `{}` | custom names by sessionId (≤ 100, ≤ 60 chars), excluded from backups | `:898` |
| `notchSessionsCostumes` | `{}` | costume codes by sessionId (≤ 100), excluded from backups | `:904` |

Settings UI (`NotchClaudeSessionsSettingsControls.swift`): the description text; four switches (live activity, auto-expand + hint, previews + hint, buddies + hint with 3 sample Clawds, dimmed when off); a divider; "Claude Code hooks" (Installed / Not installed + Install Hooks / Remove Hooks, hint text, error line); "Antigravity extension" (Install / Update, hint, error). Opening the settings re-checks the hooks and the extension.

Privacy rules (enforce them in Rust, not in the UI):
- With previews **off** (the default), nothing a session says is decoded or kept: no prompts, replies, titles, tool summaries, subagent descriptions, current task text, or background labels. The Stop hook's `last_assistant_message` is ignored. Only times, tool **names**, ids, branch, model, token usage and cost are kept (`ClaudeTranscriptReader.swift:6-9`, `ClaudeSessionEngine.swift:172-189, 259-295`).
- **Always** shown regardless of previews: a permission's full `detail` (the command/path/args, up to 8000 chars) and AskUserQuestion texts, options, descriptions and previews ("nobody can answer what they cannot read"). The detail is dropped once answered.
- Turning previews off purges memory at once. Turning them on re-reads the transcripts.
- Nothing is sent over the network. Sockets are owner-only. Settings backups are 0600 ("settings can hold tokens"). From IDE lock files only `workspaceFolders` is read, never `authToken`. From process environments only the 2 host keys are read.

---

## Open questions / risks

1. **Registry on Windows.** Does Claude Code 2.1.3xx native Windows write `%USERPROFILE%\.claude\sessions\<pid>.json` with `status`/`waitingFor`? What is `pidDomain` (probably `"win32"`; the Mac reader **rejects** anything but `"darwin"`, so the port must accept the Windows value)? Is `procStart` present, and in what format (it comes from `ps -o lstart` on Unix)? Without it, use `startedAt` with the 60 s tolerance and compare against `GetProcessTimes`. Must be verified on a real Windows machine before building §1.3.
2. **Hook shell on Windows.** Which shell runs `command` hooks (Git Bash vs cmd vs PowerShell)? This decides quoting, the marker form and whether `exit 0` works. Test with a probe hook that prints `$0`/`%COMSPEC%`.
3. **PermissionRequest parity.** Confirm that Windows Claude Code runs the PermissionRequest hook in parallel with the terminal dialog (first answer wins), kills the hook when the terminal answers, and accepts `updatedInput.answers` for AskUserQuestion. These facts were verified only on the Mac (2.1.30x).
4. **Bracketed paste into claude.exe** via ConPTY (`WriteConsoleInputW` and VS Code `sendText`) has not been tested. If ESC is taken as the Escape key, a reply could **cancel the running turn**. Test on an idle session first.
5. **Image paste by path** on Windows (`C:\…` with spaces, backslashes) has not been tested. Also find Claude Code's per-session temp folder on Windows, the equivalent of `/private/tmp/claude-<uid>/<project>/<session>`.
6. **Windows Terminal window/tab mapping**: the `GetConsoleWindow` → `GA_ROOTOWNER` trick and UIA tab selection both need verification on WT ≥ 1.18. Selecting a specific tab may be impossible; then raise only the window.
7. **`CLAUDE_CONFIG_DIR`** is ignored by the Mac code (homes are hard-coded to `~/.claude` and `~/.config/claude`). On Windows, honor `CLAUDE_CONFIG_DIR` if set (read from the user environment). The second home `.config\claude` is probably irrelevant on Windows.
8. **Path handling.** Several Mac rules assume `/` and case-sensitive paths: `projectName` (`/.claude/worktrees/`), `ClaudeIDELock.openFolder` prefix match, `AgentLogCursor.parent(of:)` (`/subagents/`), and the subagent folder join. On Windows normalize separators, compare case-insensitively and normalize the drive-letter case.
9. **Official VS Code "Claude Code" extension sessions** (registry `entrypoint` ≠ `"cli"`, if any) run in a webview, not a terminal, so replies cannot be typed. The Mac ignores `entrypoint`/`kind`. Decide whether to show them read-only.
10. **Elevated sessions.** A claude running elevated cannot be read (PEB), attached (console) or typed into (UIPI) from a non-elevated app. Show it read-only with a clear reason.
11. **Transcript first read is the whole file.** It is fine on the Mac but can be slow for multi-GB transcripts. Consider starting at the end with a bounded back-scan, accepting a missing chat history when previews are off.
12. **Antigravity extension manifest version mismatch** (`extension.vsixmanifest` 1.0.1 vs `package.json` 1.0.3). The "outdated" check reads `package.json`; the installed folder name comes from the IDE's own naming. Fix this when porting.
13. **Hash parity.** The project color and costume seeding use 64-bit FNV-1a/SplitMix64. A JS implementation must use BigInt; a `Number` approximation silently changes every costume. Keep the test vectors from §5.2 in the Windows test suite.
14. **Thinking word** is seeded from seconds since **2001-01-01** (Apple reference date), not the Unix epoch. This only matters for cross-platform parity of the word shown.
15. **Notices vs Windows focus.** Auto-expanding the island must not steal keyboard focus (the Mac opens it with `takeFocus: false`). The Tauri window must be a no-activate topmost window (`WS_EX_NOACTIVATE`), and keyboard shortcuts (⌘Y/⌘N → proposed Ctrl+Y / Ctrl+N) only apply while the island has focus.
16. Not ported or not applicable: macOS TCC-protected folder skipping (§1.8 step 4), Automation consent, the Accessibility window raise. Low Power Mode is ported as Energy saver (`power_saver.rs`): `GUID_ENERGY_SAVER_STATUS` on 24H2 and later, `GUID_POWER_SAVING_STATUS` (Battery Saver) before it. `PowerManager.EnergySaverStatus` and `SystemStatusFlag` stay Disabled/0 while Energy saver runs on AC power (checked on 26H2), so they are not used.
