# 07 — Codex CLI Sessions: data sources, approvals and replies

Status: built on both platforms. Windows: `windows/src-tauri/src/sessions/codex/` (branch feat/windows). Mac: `Sources/Vorssaint/Services/ClaudeSessions/CodexSessionClient.swift` and `CodexSessionState.swift`. Where the builds differ from this research: cost is not priced yet (§10), the island joins a thread only while it works or its page is open, and on the Mac Open finds the terminal UI by the thread's folder (the one `codex` process with a terminal there, else the only one running).
Audience: the developer adding OpenAI Codex CLI sessions to the Sessions section of the Windows island (Tauri 2 + Rust + React), next to the Claude Code sessions in `02-sessions.md`.
Scope: the interactive `codex` TUI, run natively on Windows 11 in Windows Terminal, conhost or a VS Code-family terminal. WSL is out of scope. `codex exec`, Codex Cloud and the Codex desktop app are out of scope too; §6.4 says how to exclude them.

Every claim cites one of the sources below. **[GUESS]** marks an inference that no source states. **[VERIFY]** marks something that has to be tested in the Windows VM (list at the end).

## Sources

| Short name | What | Where |
|---|---|---|
| `npm` | The installed npm package `@openai/codex@0.163.0` (launcher + native binary) | `/Users/dawidbuszta/.local/node/lib/node_modules/@openai/codex/` (`bin/codex.js`, `node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex`) |
| `help` | `codex --help`, `codex <sub> --help`, `codex app-server help daemon`, `codex features list` from that binary | run read-only on this Mac |
| `home` | Read-only look at `~/.codex/` on this Mac (rollouts dated 2026-08-24 … 2026-10-21, written by 0.155 … 0.163). `auth.json` was not opened. | `~/.codex/` |
| `src` | `github.com/openai/codex` at tag **`rust-v0.163.0`** (commit `83086188ae06`, 2026-10-15). Paths below are relative to `codex-rs/` unless they start with `scripts/` or `README.md`. | cloned read-only into the session scratchpad |
| `mac` | How the Mac app already reads Codex | `Sources/Vorssaint/Services/AgentUsage/Harness/CodexReader.swift`, `AgentCodexServer.swift`, `CodexAccountLimits.swift`, `CodexProfileLimitsService.swift`; `windows/docs/spec/03-agents.md` §3, §6 |
| `win` | The Windows engine we already have | `windows/src-tauri/src/sessions/`, `hooks/`, `hosts/` |

---

## 0. Summary

1. **Since 0.16x the interactive TUI is a client of a shared local "app-server daemon".** By default `codex` starts or reuses one background `codex app-server` per `CODEX_HOME`, and the TUI's threads run inside that daemon process, not inside the TUI process (`tui/src/startup_orchestration.rs:508-574`; `features/src/lib.rs:1029-1033`, `daemon_auto_start` Stable + on; `help`: `--no-daemon  Run without the shared background server`).
2. **The daemon listens on a same-user AF_UNIX socket**, `%USERPROFILE%\.codex\app-server-control\app-server-control.sock` on Windows. It speaks JSON-RPC over WebSocket over that socket, and it checks no protocol-level credentials: any process of the same user can connect (§3.1).
3. **Through that socket we can do everything the island needs, for sessions the TUI started**: list loaded threads, receive `thread/status/changed` broadcasts (`active` with `waitingOnApproval` / `waitingOnUserInput`, `idle`, `notLoaded`), attach to a running thread with `thread/resume`, receive its approval requests, and **answer them**. The first answer from any client wins, and the daemon then sends `serverRequest/resolved`, which closes the TUI's prompt (`app-server/src/outgoing_message.rs:336-501, 565-587`; `tui/src/app/app_server_events.rs:293-317`). Replies go through `turn/start` / `turn/steer` / `thread/queue/add`. **This is the recommended primary mechanism (§7.1, §8.1).**
4. **The fallbacks are weaker.**
   - Codex has a Claude-like **hooks** system, including a `PermissionRequest` hook that can return allow/deny. It runs *before* the TUI prompt and blocks it (serial, not parallel like Claude). It needs explicit user trust, and it can't answer questions (`request_user_input`) or MCP elicitations (§4, §7.2).
   - Typing keys into the terminal works but is risky. On Windows the TUI turns **off** VT input, so injected `ESC[200~` bracketed-paste markers become a real Esc key, which **interrupts the turn or aborts the approval** (`tui/src/tui.rs:245-251`, `tui/src/tui/windows_console.rs:9-11`) (§7.3, §8.2).
5. **Rollout JSONL files exist** (`sessions\YYYY\MM\DD\rollout-*.jsonl`), but they **do not contain approval requests, command starts or any other in-flight event** (`rollout/src/policy.rs:127-227`). They are fine for history, tokens, model, cwd, git and turn boundaries, but they can't show "waiting for approval" or "running a command".
6. **No file records the TUI's pid.** `daemon.pid` records only the daemon's. Thread → TUI-process mapping is heuristic: command line, cwd and start time of `codex.exe` processes (§6). This is the weakest part of the design.

```
 codex.exe (TUI, one per terminal tab)  ──WS over AF_UNIX──┐
   parent: node.exe (npm shim) or the shell               │
   sets terminal title "⠋ <thread> | <project>",          ▼
   "[ ! ] Action Required | …"                 codex.exe app-server --listen unix://   (daemon, detached)
                                                 │ runs every thread, hooks, notify, tools
                                                 │ writes %USERPROFILE%\.codex\sessions\…\rollout-*.jsonl,
                                                 │        state_5.sqlite, thread-writer-locks\<id>.lock
                                                 ▲
 Vorssaint island (Rust) ──WS over AF_UNIX───────┘  initialize → thread/loaded/list → thread/resume
                                                     ← thread/status/changed, item/*, turn/*, requestApproval
                                                     → {"decision":"accept"} / turn/start …
 Host/focus/reply fallback: codex.exe pid → process tree (existing hosts::locate) → WT / VS Code / conhost
```

---

## 1. Codex on Windows: install layouts, processes, CODEX_HOME

### 1.1 Install layouts

| Method | What the user runs | Native binary | Source |
|---|---|---|---|
| npm `@openai/codex` | `%APPDATA%\npm\codex.cmd` → `node.exe …\@openai\codex\bin\codex.js` → **spawns** the native exe | `%APPDATA%\npm\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe` (arm64: `codex-win32-arm64\vendor\aarch64-pc-windows-msvc\…`) | `npm` `bin/codex.js:15-23, 79-95, 241-244`; `package.json` `optionalDependencies` |
| Standalone installer (`irm https://chatgpt.com/codex/install.ps1 \| iex`) | `codex.exe` from the visible bin dir | visible bin: `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe` (or `%CODEX_INSTALL_DIR%`), which is a junction to `%CODEX_HOME%\packages\standalone\current\…` | `README.md:22-35`; `scripts/install/install.ps1:936-958, 687-726` |
| Daemon managed install | daemon only | `%CODEX_HOME%\packages\app-server-daemon\current\bin\codex.exe` | `app-server-daemon/src/managed_install.rs:20-71`; `install.ps1:941-942` |
| winget | **[VERIFY]** package id and path | `scripts/build_winget_package.py` exists (entrypoint `bin/codex.exe`) | `scripts/build_winget_package.py:15-21` |

Consequences for us:
- The process that runs the TUI is always an image named **`codex.exe`**. With npm, its parent is `node.exe`, whose parent is `cmd.exe` (from `codex.cmd`) **[VERIFY: npm's shim on Windows]**, whose parent is the user's shell. The existing `hosts::process::ancestors` walk (`win` `hosts/process.rs:15`) handles this as long as we start from the `codex.exe` pid and keep the whole chain.
- The node launcher forwards signals and mirrors the exit code (`bin/codex.js:258-298`). It sets `CODEX_MANAGED_PACKAGE_ROOT` and one of `CODEX_MANAGED_BY_{NPM,BUN,PNPM,VITE_PLUS}` in the child's environment (`:225-240`). These are useful markers if we read the PEB environment.
- The 03-agents §3.2 candidate list for the `codex` executable on Windows should become: `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe`, `%CODEX_HOME%\packages\standalone\current\bin\codex.exe`, `%APPDATA%\npm\codex.cmd`, then `where codex`.

### 1.2 CODEX_HOME

- `CODEX_HOME` if set (it must exist and be a directory, and it is canonicalized), else `dirs::home_dir()\.codex`, i.e. **`%USERPROFILE%\.codex`** (`utils/home-dir/src/lib.rs:6-60`).
- The island reads `CODEX_HOME` from the user environment (`HKCU\Environment`), not from its own process, and also supports the multi-account homes from 03-agents §3.5 (`%USERPROFILE%\.codex-*`). **Each home has its own daemon and socket**, so the Codex session source is one instance per home.

### 1.3 Elevation and other "no daemon" cases

The TUI runs **embedded** (threads inside the TUI process, no socket we can join) when any of these apply (`tui/src/daemon_startup.rs:65-128`, `tui/src/startup_orchestration.rs:26-36, 505-595`):
- `--no-daemon`, `--oss`, `--profile`, workload identity, `CODEX_EXEC_SERVER_URL`, `--strict-config`, `--dangerously-bypass-hook-trust`, or any `-c/--enable/--disable` override outside an allow-list;
- **an elevated (Administrator) terminal on Windows**. It prints "Running as administrator: shared background server disabled…" (`daemon_startup.rs:56`);
- a launcher job object that forbids breakaway (`startup_orchestration.rs:557-576`, `app-server-daemon/src/backend/windows.rs:126-170`);
- a daemon feature-compatibility mismatch.

The engine must support these embedded sessions in a degraded, read-mostly mode (§5.3).

---

## 2. On-disk data (read-only)

### 2.1 `%CODEX_HOME%` layout seen on this Mac (`home`) and what each item is for

| Path | Use for us | Notes |
|---|---|---|
| `config.toml` | `model`, `model_reasoning_effort`, `notify`, `[hooks]`, `[tui]` (`notifications`, `terminal_title`, `disable_paste_burst`), `[projects."<path>"].trust_level`, `[mcp_servers.*]` | **May hold secrets** (MCP `env_http_headers`, e.g. API keys on this Mac). Parse only the keys we need and never log the file. |
| `auth.json` | **never read** | credentials |
| `sessions\YYYY\MM\DD\rollout-YYYY-MM-DDTHH-MM-SS-<threadId>.jsonl` | history, tokens, model, cwd, git, turn boundaries (§2.2) | 61 files here; `<threadId>` is a UUIDv7 |
| `archived_sessions\…` | same parser (03-agents §3.1) | |
| `state_5.sqlite` (+ `-wal`, `-shm`) | thread index: `threads` table (§2.3) | sqlx migrations; the version suffix will change |
| `thread_history_1.sqlite` | paginated turn/item projection of rollouts (`thread_turns`, `thread_items`) | not needed if we use the daemon |
| `thread-writer-locks\<threadId>.lock`, `.coordination.lock` | "a writer currently owns this thread" (§2.4) | empty files plus OS locks |
| `app-server-control\app-server-control.sock`, `app-server-startup.lock` | the daemon socket (§3) | `app-server-transport/src/transport/mod.rs:53-78` |
| `app-server-daemon\daemon.pid`, `daemon.lock`, `settings.json`, `loaded-threads.json`, `daemon.stderr.log` | daemon liveness and its pid (§3.1) | `app-server-daemon/src/lib.rs:54-60, 323-347`; `backend/pid.rs:46-57` |
| `session_index.jsonl` | `{"id","thread_name","updated_at"}` per renamed thread | thread names (content: previews only) |
| `history.jsonl` | `{"session_id","ts","text"}` per submitted prompt | **conversation content**; not needed, don't read |
| `queue_1.sqlite` | durable `thread/queue/add` items | not needed |
| `logs_2.sqlite`, `goals_1.sqlite`, `memories_1.sqlite`, `shell_snapshots\`, `tmp\arg0*\` | internal | ignore |

**SQLite caution.** Opening a WAL database, even with `?mode=ro`, can create the `-shm`/`-wal` side files, and it takes shared locks. When the research for this doc ran `sqlite3 -readonly`, `-shm`/`-wal` files appeared next to `goals_1`, `logs_2`, `queue_1` and `thread_history_1` (`home`, timestamps). Prefer the daemon's `thread/list` over reading `state_5.sqlite`. If we must read it, open it with `SQLITE_OPEN_READONLY`, never `immutable=1` (that is wrong for a live DB), and never write.

### 2.2 Rollout JSONL

Every line is `{"timestamp":"<ISO-8601 Z>","ordinal":<int>,"type":"<kind>","payload":{…}}` (`home`). The Mac parser prefilters on the first `"type"` within the first 64 bytes (03-agents §3.1); `ordinal` is new and must be tolerated.

Line kinds counted in one TUI rollout on this Mac (1146 lines): `event_msg/item_completed` 345, `event_msg/token_count` 139, `token_usage_record` 123, `response_item/*` ≈ 430, `inter_agent_communication_metadata` 52, `world_state` 22, `turn_context` 10, `event_msg/task_started` 9, `event_msg/thread_settings_applied` 9, `event_msg/task_complete` 8, `session_meta` 1, `compacted` 1, `event_msg/turn_aborted` 1.

Shapes (values replaced by placeholders):

```jsonc
// first line; later session_meta lines are copied history (03-agents §3.1)
{"type":"session_meta","payload":{
  "id":"<threadId>","session_id":"<rootThreadId>","timestamp":"…","cwd":"<abs path>",
  "originator":"codex-tui",            // codex_exec for `codex exec`
  "cli_version":"0.163.0","source":"cli",   // or {"subagent":{"thread_spawn":{"parent_thread_id":"…","depth":1,"agent_path":"/root/<name>","agent_nickname":"…","agent_role":null}}}
  "thread_source":"user","model_provider":"openai","history_mode":"paginated",   // "legacy" in older files
  "base_instructions":{…}, "context_window":{"window_id":"…"},
  "git":{"commit_hash":"<40 hex>","branch":"<branch>","repository_url":"<url>"}}}   // git absent outside a repo

{"type":"turn_context","payload":{"turn_id":"…","cwd":"…","approval_policy":"on-request","approvals_reviewer":"user",
  "sandbox_policy":{"type":"workspace-write",…},"model":"gpt-5.6-sol","collaboration_mode":{"mode":"default","settings":{"model":"…","reasoning_effort":"high",…}},…}}

{"type":"event_msg","payload":{"type":"thread_settings_applied","thread_id":"…","thread_settings":{"model":"…","service_tier":"<tier>","approval_policy":"…","cwd":"…","reasoning_effort":"…",…}}}
{"type":"event_msg","payload":{"type":"task_started","turn_id":"…","started_at":<unix s>,"model_context_window":<int>,"collaboration_mode_kind":"default"}}
{"type":"event_msg","payload":{"type":"task_complete","turn_id":"…","last_agent_message":"<text>","started_at":…,"completed_at":…,"duration_ms":…,"time_to_first_token_ms":…}}
{"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"…","reason":"interrupted","started_at":…,"completed_at":…,"duration_ms":…}}

{"type":"token_usage_record","payload":{"thread_id":"…","turn_id":"…","session_id":"…","root_turn_id":"…","response_id":"<id>",
  "usage":{"input_tokens":0,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":0},
  "turn_token_usage":{…same…},"thread_token_usage":{…same…}}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{…},"last_token_usage":{…},"model_context_window":<int>},
  "rate_limits":{"limit_id":"codex","primary":{"used_percent":…,"window_minutes":300,"resets_at":…},"secondary":{…},"credits":{…},"plan_type":"plus",…}}}

// paginated history: one item_completed per finished item (started items are NOT written)
{"type":"event_msg","payload":{"type":"item_completed","thread_id":"…","turn_id":"…","started_at_ms":…,"completed_at_ms":…,
  "item":{"type":"CommandExecution","id":"…","process_id":"<string>","command":["<shell>",…],"cwd":"…",
          "parsed_cmd":[{"type":"read","cmd":"…","name":"<file>","path":"…"}],"source":"…","status":"completed",
          "exit_code":0,"duration":{"secs":0,"nanos":0},"aggregated_output":"<text>",…}}}
```

`item.type` values seen: `UserMessage`, `AgentMessage` (`phase`: `commentary`/…), `Reasoning`, `CommandExecution`, `FileChange` (`changes` keyed by path), `McpToolCall` (`server`, `tool`, `status`, `duration`), `Extension` (`kind` e.g. `web.search`), `ImageView`, `SubAgentActivity` (`kind` `started`…, `agent_thread_id`), `CollabAgentToolCall`, `ContextCompaction` (`home`). Function-call names seen in `response_item`: `exec`, `spawn_agent`, `wait_agent`, `send_message`, `followup_task`, `list_agents`, `wait`, `sleep`.

**What is never persisted** (`rollout/src/policy.rs:161-224`, "Transient, non-durable events"): `ExecApprovalRequest`, `ApplyPatchApprovalRequest`, `RequestPermissions`, `RequestUserInput`, `ElicitationRequest`, `ExecCommandBegin`, `ExecCommandOutputDelta`, `ItemStarted`, `McpToolCallBegin`, `PatchApplyBegin`, `HookStarted/Completed`, `Error`, `SessionConfigured`, `ShutdownComplete`, all deltas. Persisted: `ItemCompleted` (paginated mode; command output is truncated, `:250-282`), `TokenCount`, `TurnStarted` (`task_started`), `TurnComplete` (`task_complete`), `TurnAborted`, `ThreadSettingsApplied`, `ThreadGoalUpdated`, `ThreadRolledBack` (`:132-139`). In `legacy` history mode the old `user_message`/`agent_message`/… events are written instead (`:141-158`).

⇒ From rollouts alone: **working** = a `task_started` without a matching `task_complete`/`turn_aborted`; **done** = `task_complete`; tools appear only **after** they finish. **"Waiting for approval", "waiting for input" and "running a command" cannot be derived from rollouts.**

The pid is **not** in the rollout. `CommandExecution.process_id` is the id of the unified-exec process Codex spawned (here `"71638"`), not Codex's own pid **[GUESS from the value and the field name; not traced]**.

### 2.3 `state_5.sqlite` → `threads`

Useful columns (`home` schema): `id`, `rollout_path`, `created_at[_ms]`, `updated_at[_ms]`, `source` (`cli` / `exec` / JSON for subagents), `originator` (`codex-tui`, `codex_exec`, NULL in older rows), `model_provider`, `model`, `reasoning_effort`, `cwd`, `title`, `name`, `preview`, `first_user_message` (**content**), `approval_mode`, `sandbox_policy`, `tokens_used`, `git_sha`, `git_branch`, `git_origin_url`, `cli_version`, `history_mode`, `archived`, `agent_nickname`, `agent_role`, `agent_path`. Also `thread_spawn_edges(parent_thread_id, child_thread_id, status)` for subagents.

Counts here: `cli/codex-tui` and `cli/NULL` (older) for TUI threads, `exec/codex_exec` for `codex exec`. This is the same data the daemon's `thread/list` serves (§3.4), so use the daemon and keep SQLite only as an embedded-mode fallback **[proposal]**.

### 2.4 `thread-writer-locks\<threadId>.lock`

- Created on `acquire` (the process that will write the thread's rollout), held with `File::try_lock` (an exclusive `LockFileEx` on Windows), and **deleted on drop** (`rollout/src/writer_lock.rs:43-85, 177-196`). Stale files (owner crashed) are removed by the next writer's sweep (`:123-174`). The file is empty; no pid inside.
- In daemon mode the lock holder is the **daemon**, not the TUI **[GUESS: the writer is wherever the thread runs; consistent with §1]**.
- Use: the file's **existence** is a cheap "this thread is (or very recently was) live" signal. **Do not** probe it with `try_lock`: a probe that succeeds owns the lock for a moment, and a Codex starting that thread at the same instant would fail with "already has an active writer" (`:66-71`). Codex itself probes only while holding `.coordination.lock` (`:91-105`), which we must not take either.

---

## 3. The live channel: the app-server daemon

### 3.1 Finding and trusting the daemon

1. The daemon is running when `%CODEX_HOME%\app-server-daemon\daemon.pid` exists, its JSON `{"pid":u32,"processStartTime":"…","processIdentity"?,"executableIdentity"?}` names a live process whose start time matches (`app-server-daemon/src/backend/pid.rs:46-57`), and `…\app-server-control\app-server-control.sock` exists. A legacy standalone install writes `app-server.pid` instead (`lib.rs:54-60, 330`). Its command line is `codex app-server --listen unix:// --analytics-default-enabled [-c features.X=…]` (`backend/pid.rs:383-429`). It runs detached (`DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB`, `backend/windows.rs:141-170`).
2. **Windows transport: a real AF_UNIX socket** (crate `uds_windows`), **not a named pipe** (`uds/src/lib.rs:185-249`). The directory `app-server-control` carries the protected DACL `O:<sid>D:P(A;OICI;FA;;;<sid>)` (`uds/src/windows_security.rs:24-90`). On macOS/Linux the advertised path is a symlink to `/tmp/codex-daemon-<euid>/<sha256>` (`uds/src/daemon_directory.rs:13-39`), which is irrelevant to Windows.
3. **No authentication** beyond the filesystem ACL: control-socket connections open with `auth: None` (`app-server-transport/src/transport/websocket.rs:186-193`). The official client checks the **server** with `SIO_AF_UNIX_GETPEERPID`: same user SID, and neither side elevated (`uds/src/windows_peer.rs:25-103`, called from `app-server-client/src/remote.rs:808-812`). **We should do the same check** before trusting what the socket says, and run the island non-elevated.
4. **We never start the daemon ourselves.** The TUI starts it on demand. If there is no daemon, there are no daemon-mode sessions, and we drop to §5.3. Starting one (`codex app-server daemon start`) would change the user's Codex setup **[proposal]**.
5. Path length: Windows AF_UNIX `sun_path` holds 108 bytes. `C:\Users\<name>\.codex\app-server-control\app-server-control.sock` fits for normal names. A long `CODEX_HOME` might not **[VERIFY: what Codex does then]**.

### 3.2 Framing and handshake

- **WebSocket over the UDS stream.** The client handshakes to `ws://localhost/rpc` (`app-server-client/src/remote.rs:70, 781-830`; server `transport/unix_socket.rs:176-211`). Each JSON-RPC message is one WS **Text** frame. `/daemon/shutdown` is reserved for the managed daemon (`unix_socket.rs:180-187`).
- The JSON-RPC dialect has **no `"jsonrpc":"2.0"` field**. Shapes: request `{id, method, params?}`, notification `{method, params?}`, response `{id, result}`, error `{id, error:{code,message,data?}}`; `id` is a string or an integer (`app-server-protocol/src/rpc.rs:1-88`). Notifications also carry `emittedAtMs` (`protocol/common.rs:2079-2095`). The Mac app already speaks this dialect over **stdio NDJSON** to a private `codex app-server` (`mac` `AgentCodexServer.swift:183-216`). Only the transport differs.
- Handshake (`remote.rs:844-987`; `protocol/v1.rs:27-85`):

```json
→ {"id":1,"method":"initialize","params":{"clientInfo":{"name":"vorssaint","title":"Vorssaint","version":"<app version>"},
     "capabilities":{"experimentalApi":true}}}
← {"id":1,"result":{"userAgent":"…","codexHome":"C:\\Users\\…\\.codex","platformFamily":"…","platformOs":"…"}}
→ {"method":"initialized"}
```

- `experimentalApi:true` is required. Without it experimental methods are rejected (`app-server/src/message_processor.rs:980`), experimental notifications are dropped and some approval fields are stripped (`app-server/src/transport.rs:104-125, 184-193`).
- **Version pinning.** The protocol is versioned only by the binary. At startup, record `userAgent` (it contains the version) and parse leniently: ignore unknown fields and unknown methods. `codex app-server generate-json-schema` (`help`) can dump the exact schema of the installed version for tests.

### 3.3 Methods we use

Conversation history is requested only with Show previews enabled. Enabling previews reloads known threads; disabling it clears cached conversation messages and ignores late message content. History and live `item/started` / `item/completed` messages are merged by item id, retaining only the latest 100 messages and shortening each after 16,000 characters. A read captures the message revision when sent: notifications received after that request take precedence over its history, while a later read can complete an earlier partial message. Token deltas remain disabled. The compact view uses the latest assistant text; the enlarged view uses the same conversation and marks `final_answer` messages as turn summaries.

| Purpose | Method | Params → result | Ref |
|---|---|---|---|
| Loaded (live) thread ids | `thread/loaded/list` | `{cursor?, limit?}` → `{data:[threadId], nextCursor}` | `v2/thread.rs:1659-1677` |
| Thread metadata + status | `thread/read` | `{threadId, includeTurns:previews}` → `{thread: Thread}` | `v2/thread.rs:1705-1723` |
| Recent threads (picker, history) | `thread/list` | `{cursor, limit, sortKey, sortDirection, sourceKinds, archived, cwd, searchTerm, …}`; omitted `sourceKinds` = interactive sources | `v2/thread.rs:1403-1480` |
| Attach to a running thread | `thread/resume` | `{threadId, excludeTurns:!previews}`, **no overrides** | `request_processors/thread_processor.rs:4499-4700` |
| Detach | `thread/unsubscribe` | `{threadId}` → `{status:"notLoaded"\|"notSubscribed"\|"unsubscribed"}` | |
| Send a prompt (idle) | `turn/start` | `{threadId, input:[UserInput], clientUserMessageId?}` | `v2/turn.rs:170-215` |
| Add to the running turn | `turn/steer` | `{threadId, input, expectedTurnId}` → `{turnId}` | `v2/turn.rs:311-340` |
| Queue for later (what `codex queue` does) | `thread/queue/add` (experimental) | `{threadId, input:[{"type":"text","text":"…","text_elements":[]}], clientUserMessageId:"<uuidv7>"}` | `tui/src/session_queue_commands.rs:27-147`; `v2/thread.rs:926-947` |
| Stop the turn | `turn/interrupt` | `TurnInterruptParams` **[VERIFY field names]** | `schema/typescript/v2/TurnInterruptParams.ts` |
| Account limits (already used) | `account/read`, `account/rateLimits/read` | 03-agents §3.3 | `mac` |

`Thread` fields (`schema/typescript/v2/Thread.ts`): `id`, `sessionId`, `forkedFromId`, `parentThreadId`, `preview` (content), `name`, `ephemeral`, `historyMode`, `modelProvider`, `model`, `reasoningEffort`, `createdAt`, `updatedAt`, `recencyAt`, `status`, `path` (rollout path), `cwd`, `cliVersion`, `originator`, `source`, `threadSource`, `agentNickname`, `agentRole`, `gitInfo:{sha,branch,originUrl}`, `turns`.

`ThreadStatus` = `{"type":"notLoaded"} | {"type":"idle"} | {"type":"systemError"} | {"type":"active","activeFlags":["waitingOnApproval"|"waitingOnUserInput"]}` (`ThreadStatus.ts`, `ThreadActiveFlag.ts`).

`UserInput` is tagged by `"type"`: `text {text, text_elements}`, `image`, `localImage {path}`, `skill`, `mention`, … (`v2/turn.rs:443-475`).

### 3.4 Notifications we consume

- **Broadcast to every initialized connection**, subscribed or not: `thread/status/changed {threadId, status}` (`app-server/src/thread_status.rs:223-249`; `outgoing_message.rs:747-801`). This alone drives working / waiting / idle for every loaded thread without attaching.
- **To subscribers of a thread** (after `thread/resume`):
  - `turn/started`, `turn/completed {threadId, turn:{id, status:"completed"|"interrupted"|"failed"|"inProgress", error, startedAt, completedAt, …}}`
  - `item/started` / `item/completed {item: ThreadItem, threadId, turnId, startedAtMs}`. `ThreadItem.type` ∈ `userMessage, agentMessage, reasoning, commandExecution, fileChange, mcpToolCall, dynamicToolCall, webSearch, imageView, imageGeneration, plan, subAgentActivity, collabAgentToolCall, contextCompaction, enteredReviewMode, exitedReviewMode, functionCallOutput, hookPrompt, sleep`. `commandExecution` has `command` (string), `cwd`, `status` (`inProgress|completed|failed|declined`), `exitCode`.
  - `thread/tokenUsage/updated {threadId, turnId, tokenUsage:{total, last, modelContextWindow}}`, each breakdown `{totalTokens, inputTokens, cachedInputTokens, cacheWriteInputTokens, outputTokens, reasoningOutputTokens}`
  - `serverRequest/resolved {threadId, requestId}`, `thread/closed`, `error`, deltas (`item/agentMessage/delta`, `item/commandExecution/outputDelta`, …), `turn/plan/updated`, `turn/diff/updated`.
  (`protocol/common.rs:1948-2077`; types in `schema/typescript/v2/*.ts`.)

### 3.5 Server → client requests (approvals and questions) and their answers

All are JSON-RPC requests with an `id` that we answer with `{"id":<same>,"result":{…}}`.

| Request | Key params | Answers |
|---|---|---|
| `item/commandExecution/requestApproval` (also covers network access and `write_stdin`) | `kind` (`command`\|`writeStdin`), `threadId`, `turnId`, `itemId`, `startedAtMs`, `approvalId?`, `reason?`, `command?`, `cwd?`, `commandActions?`, `networkApprovalContext?`, `proposedExecpolicyAmendment?`, `proposedNetworkPolicyAmendments?`, `availableDecisions?` (experimental) | `{"decision":"accept"}` · `{"decision":"acceptForSession"}` · `{"decision":{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["cmd","arg"]}}}` · `{"decision":{"applyNetworkPolicyAmendment":{"network_policy_amendment":…}}}` · `{"decision":"decline"}` (deny, turn continues) · `{"decision":"cancel"}` (deny + interrupt turn) — `v2/item.rs:64-82, 1555-1633`; inner fields snake_case per `schema/typescript/v2/CommandExecutionApprovalDecision.ts:7` |
| `item/fileChange/requestApproval` | `threadId`, `turnId`, `itemId`, `startedAtMs`, `reason`, `grantRoot` | `{"decision":"accept"\|"acceptForSession"\|"decline"\|"cancel"}` — `v2/item.rs:113-124, 1638-1658` |
| `item/permissions/requestApproval` | `threadId`, `turnId`, `itemId`, `cwd`, `reason`, `permissions:{network?,fileSystem?}` | grant `{"permissions":{…subset…},"scope":"turn"\|"session","strictAutoReview"?:bool}`; deny `{"permissions":{}}` **[inferred from `bespoke_event_handling.rs:1865-1919`; VERIFY]** — `v2/permissions.rs:773-806` |
| `item/tool/requestUserInput` | `threadId`, `turnId`, `itemId`, `questions:[{id,header,question,isOther,isSecret,options?}]`, `isBlocking` | `{"answers":{"<questionId>":{"answers":["<label or text>"]}}}` — `v2/item.rs:1740-1810` |
| `mcpServer/elicitation/request` | `threadId`, `turnId?`, `serverName`, `mode` = `form {message, requestedSchema}` \| `url {message, url, elicitationId}` \| `openai/form` \| `openai/userVerification` | `{"action":"accept"\|"decline"\|"cancel","content":<object\|null>,"_meta":null}` — `v2/mcp.rs:380-436, 783-912`. `openai/userVerification` goes to **one owning connection only** and others cannot answer it (`outgoing_message.rs:365-381, 567-587`). Show it read-only. |

Also seen but not for us: `item/tool/call`, `account/chatgptAuthTokens/refresh`, `attestation/generate`, `currentTime/read`, legacy v1 `execCommandApproval`/`applyPatchApproval`. **Never answer requests we don't understand**. Leave them to the TUI.

### 3.6 Routing: who gets the request, who may answer

- A request is sent to **every connection subscribed to the thread** (`request_processors/thread_lifecycle.rs:356-365`; `outgoing_message.rs:336-450`, loop `:418-436`). If we subscribe later, `thread/resume` **replays pending requests** to us (`thread_lifecycle.rs:961-964`; `outgoing_message.rs:452-471`).
- **First answer wins.** `take_connection_callback` removes the pending entry for whoever answers first, with no recipient check (except `userVerification`) (`outgoing_message.rs:565-587`). A later answer only logs "could not find callback" (`:473-501`).
- The daemon then emits `serverRequest/resolved`, and the TUI calls `dismiss_app_server_request` to close its overlay (`tui/src/app/app_server_events.rs:293-317`). The reverse also holds: when the user answers in the terminal, we receive `serverRequest/resolved` and must drop our card.
- A turn transition cancels any pending request for the thread (`outgoing_message.rs:221-236, 604-637`), so also drop the card on `turn/completed`.

This is exactly the "parallel, first answer wins" behaviour the Claude `PermissionRequest` hook gives us on the Mac (02-sessions §2.8).

### 3.7 Side effects of attaching (keep them small)

- **Keeps threads loaded.** The daemon unloads a thread after `thread_unload_delay` once it has **no subscribers and is not active** (`request_processors/thread_lifecycle.rs:23-110`). If we stay subscribed after the user closes the TUI, the thread stays loaded and looks live. **Rule: subscribe (`thread/resume`) only while the thread is `active`, or while its TUI process is alive and the session page is open. Unsubscribe when it goes `idle` and the page is closed.** Status for unsubscribed threads still arrives through the `thread/status/changed` broadcast.
- **Overwrites the recorded client name.** `thread/resume` and `turn/start` call `set_app_server_client_info` with our `clientInfo` (`thread_processor.rs:4665-4670, 1002-1018`; `turn_processor.rs:1030-1040`). Known effects: the `client` field of the legacy notify payload, and an Xcode-specific elicitation auto-deny (`xcode_26_4_mcp_elicitations_auto_deny`). It returns to `codex-tui` on the TUI's next `turn/start` **[GUESS]**. Don't impersonate `codex-tui`. Accept this and note it in Open questions.
- **Never send override fields** (model, cwd, approval policy) on `thread/resume`. With other subscribers they are ignored with a warning (`thread_processor.rs:4566-4615`); without them they would change the user's thread.
- Connection count and daemon lifetime: whether a connected idle client keeps the daemon from shutting down is **[VERIFY]**. A resident island connection could keep the daemon alive indefinitely. If that matters, disconnect when no Codex TUI process exists.

---

## 4. Hooks and `notify` (fallback channel)

### 4.1 Hooks feature (`hooks` = Stable, on by default: `features/src/lib.rs:1346-1349`)

- **Events**: `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreCompact`, `PostCompact`, `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `SubagentStart`, `SubagentStop`, `Stop`, `Interrupt` (`config/src/hook_config.rs:36-61`). **No `Notification` event.**
- **Where they are configured**: `%CODEX_HOME%\hooks.json` or `[hooks]` in `%CODEX_HOME%\config.toml` (user layer); `<project>\.codex\hooks.json` (only for trusted projects); system/managed layers; plugins (`hooks/src/engine/discovery.rs:129-400`; `config/src/state.rs:219-243`).

```toml
[[hooks.PermissionRequest]]
matcher = "*"
[[hooks.PermissionRequest.hooks]]
type = "command"
command = "vorssaint-hook"                      # non-Windows
commandWindows = 'C:\Users\<u>\AppData\Local\Programs\Vorssaint\vorssaint-hook.exe --codex --marker vorssaint-hook'
timeout = 600                                    # seconds, default 600, min 1, no max
statusMessage = "Waiting for the island"
```

- **Trust gate**: user/project/plugin hooks run only once trusted. Trust is `hooks.state."<source path>:<event_snake>:<group>:<handler>".trusted_hash = "sha256:<hash of normalized handler>"` in the **user** `config.toml` (`discovery.rs:715-813`; `hooks/src/config_rules.rs:8-29`; `hooks/src/lib.rs:94-123`). The TUI shows "Hooks need review" at startup with "Review hooks" / "Trust all and continue" / "Continue without trusting" (`tui/src/startup_hooks_review.rs:71-81, 238-276`), and `/hooks` manages them. **We must not write `trusted_hash` silently.** Install the hook, then tell the user that Codex will ask them to trust it. Any edit to the hook flips it to "Modified" and it stops running until trusted again.
- **Payload on stdin** (`hooks/src/schema.rs`): every event has `session_id`, `transcript_path` (the rollout path), `cwd`. `PermissionRequest` (`:301-318`): `session_id, turn_id, agent_id?, agent_type?, transcript_path, cwd, hook_event_name, model, permission_mode ("default"|"bypassPermissions"), tool_name, tool_input`. `tool_name` is `"Bash"` for exec (`tool_input {command, description?}`, network approvals: description `network-access <target>`) and `"apply_patch"` for edits (`tool_input {command:<patch>}`). Others cover MCP tools, `write_stdin` and `request_permissions` (`core/src/tools/approvals.rs:155-216`). **No pid, no tool_use_id.**
- **Output contract (PermissionRequest)**: stdout `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"|"deny","message":"…"}}}`; exit 0 with empty stdout = no decision; exit 2 + stderr = deny. Unknown fields are rejected (`schema.rs:186-226`; `hooks/src/events/permission_request.rs:149-279`). `allow` = one-time approval (no "for session"); there is no "ask".
- **Ordering — the big difference from Claude**: `request_approval` runs the PermissionRequest hooks **first and awaits them**. Only with no decision does it ask Guardian or the user (`core/src/tools/approvals.rs:463-483`, comment "Approval precedence is: 1. Hooks 2. … Else, user"). While our hook waits, **the TUI shows no approval prompt**, only the hook's `statusMessage`. A hook that waits for the island therefore takes the terminal's ability to answer away.
- Not hookable: `request_user_input` questions and MCP elicitations (only `approvals.rs:466` calls the hook runner).
- **Spawn on Windows**: `commandWindows` replaces `command` (`discovery.rs:515-519`). It runs via the session shell, PowerShell by default: `<pwsh|powershell>.exe -NoProfile -Command <command>` (`core/src/shell.rs:34-43`; `shell-command/src/shell_detect.rs:363-366`), else `%COMSPEC% /C` (`hooks/src/engine/command_runner.rs:384-462`). It gets `CREATE_NO_WINDOW` and a Job Object (killed on timeout, `:276-357`), and cwd = payload `cwd`. The environment is a credential-stripped snapshot of the Codex process. **No `CODEX_THREAD_ID`** is added for hooks (`hooks/src/registry.rs:79`; `command_runner.rs:416-435`). In daemon mode the hook's ancestor is the daemon, not the TUI **[GUESS: hooks run where the session core runs]**, so a hook cannot locate the terminal by walking its parents the way the Claude hook can.

### 4.2 Legacy `notify`

`notify = ["C:\\…\\vorssaint-hook.exe", "--codex-notify"]` at the top level of `config.toml` (`config/src/config_toml.rs:245-247`). It fires once per completed turn, after Stop hooks (`core/src/session/turn.rs:768-776`), with **no trust gate and no feature flag** (`hooks/src/registry.rs:124-129`). The JSON is the **last argv argument** (stdin/stdout null, fire-and-forget, no shell, `CREATE_NO_WINDOW`) (`hooks/src/legacy_notify.rs:13-100`):

```json
{"type":"agent-turn-complete","thread-id":"<uuid>","turn-id":"…","cwd":"…","client":"codex-tui","input-messages":["<content>"],"last-assistant-message":"<content>"}
```

Only one `notify` program can be set, and users often already use it for desktop toasts. Installing ours means **chaining** to the previous value. It gives "done" only, never "waiting". **[proposal: don't install notify; the daemon and rollout cover "done"]**

---

## 5. Phases

Phases are the existing `SessionPhase` set (`win` `sessions/models.rs:33`; 02-sessions §3.1): `generating`, `runningTool`, `waitingPermission`, `waitingInput`, `background`, `finished`, `idle`.

### 5.1 Daemon mode (preferred; evaluated in this order)

| # | Condition | Phase | Data |
|---|---|---|---|
| 1 | We hold an unresolved `item/commandExecution/requestApproval`, `item/fileChange/requestApproval` or `item/permissions/requestApproval` for the thread | `waitingPermission` (card from the request) | §3.5 |
| 2 | We hold `item/tool/requestUserInput` or `mcpServer/elicitation/request` | `waitingInput` (question form) | §3.5 |
| 3 | `status.type=="active"` and `activeFlags` ∋ `waitingOnApproval` (not subscribed yet, or the request is still in flight) | `waitingPermission` without detail; subscribe now so the request is replayed | `thread/status/changed` |
| 4 | `activeFlags` ∋ `waitingOnUserInput` | `waitingInput` | same |
| 5 | `active`, and an `item/started` of a tool-like type (`commandExecution`, `fileChange`, `mcpToolCall`, `dynamicToolCall`, `webSearch`, `imageGeneration`, `collabAgentToolCall`) has no `item/completed` yet | `runningTool`; `currentTool` = that item | `item/*` |
| 6 | `active`, `subAgentActivity` children still running and the main turn only waits (`collabAgentToolCall` wait) | `background` **[proposal]** | `item/*`, `Thread.parentThreadId` |
| 7 | `active` otherwise | `generating` | |
| 8 | `idle`, the last `turn/completed` had `status=="completed"`, < 10 min ago, not acknowledged | `finished` | `turn/completed` |
| 9 | `idle` / `systemError` otherwise | `idle` (`systemError` shows an error badge **[proposal]**) | |
| — | `notLoaded`, or `thread/closed` | session ends; remove it after the TUI process is gone (§6) | |

Without a subscription we lack rule 5 and turn outcomes. The plan is: be subscribed whenever the thread is `active` (§3.7), and use `thread/read` for the last turn's status when it goes idle **[VERIFY: `thread/read` with `includeTurns:false` still reports the last turn status; otherwise use `includeTurns:true` with a small page]**.

`turn/completed.status=="interrupted"` → `idle` (Esc in the TUI), like Claude's interrupt. `"failed"` → `finished` with an error flag **[proposal]**.

### 5.2 Fields per session

| Field | Daemon source | Rollout fallback |
|---|---|---|
| id | `Thread.id` | `session_meta.id` / file name |
| folder (`cwd`, project) | `Thread.cwd` | `session_meta.cwd`, then `turn_context.cwd` |
| git branch | `Thread.gitInfo.branch` (snapshot at thread start) | `session_meta.git.branch` |
| name/title | `Thread.name` (user rename, `session_index.jsonl`), else folder; `preview` only with previews on | same |
| model | `Thread.model` (+ `reasoningEffort`) | last `turn_context.model` |
| tokens | `thread/tokenUsage/updated.tokenUsage.total` | `token_usage_record.thread_token_usage`, else 03-agents §3.1 rules |
| cost | price `total` via `AgentPricing` (§10) | same |
| recent tools | `item/started`/`item/completed`, ring of 24 → 12 like Claude | `item_completed` (finished only) |
| subagents | threads with `parentThreadId == id` (`source.subagent`) | `session_meta.source.subagent`, `thread_spawn_edges` |
| started / last active | `Thread.createdAt`, `updatedAt`, `turn.startedAt/completedAt` | `task_started.started_at`, `task_complete.completed_at` |

Tool summary text (with previews on): `commandExecution.command`; `fileChange` → file names; `mcpToolCall` → `server.tool`; `webSearch` → query. With previews off, keep the type name only ("Command", "Edit", "MCP", "Search"), following the 02-sessions §7 privacy rules. Permission detail (`command`, `cwd`, `reason`, patch paths) is always shown while the card is open, as for Claude.

### 5.3 Embedded mode (no daemon: elevated, `--no-daemon`, `-c` overrides…)

- Discover live sessions by `thread-writer-locks\*.lock` existence ∧ a `codex.exe` TUI process (§6) ∧ a rollout modified recently. Tail that rollout (`Thread.path` / `rollout_path`).
- Phases: `task_started` open → `generating`; `task_complete` → `finished`; `turn_aborted` → `idle`. **No waiting phases**, unless:
  - the terminal title says `[ ! ] Action Required` (§9.2), read via UI Automation from the WT tab or window title → `waitingPermission` without detail **[proposal; VERIFY]**; or
  - the user enabled the PermissionRequest hook (§7.2).
- Replies, approvals: keystroke fallback only (§7.3, §8.2), and only on an explicit click.
- An elevated `codex.exe` cannot be read or typed into from a non-elevated island anyway (02-sessions Open question 10). Show it read-only.

---

## 6. Detecting sessions and mapping them to a process

### 6.1 What exists

- Threads: from the daemon (`thread/loaded/list` + `thread/read`), or from rollouts/locks in embedded mode.
- Processes: `CreateToolhelp32Snapshot` (already in `win` `hosts/process.rs`), image `codex.exe`.
- **Nothing links them directly.** No pid in rollouts, hooks, notify or the protocol. `daemon.pid` gives the daemon's pid only, and `server/diagnostics` returns only the daemon's process id (`v2/diagnostics.rs:14-25`). The writer lock file has no pid (§2.4).

### 6.2 Classify `codex.exe` processes (read the command line from the PEB, like 02-sessions §6.2.1 step 5)

- **Daemon**: pid = `daemon.pid`, or argv contains `app-server`. Not a session.
- **Not interactive**: argv[1] ∈ `exec`/`e`, `review`, `mcp-server`, `mcp`, `app-server`, `exec-server`, `login`, `logout`, `queue`, `doctor`, `features`, `debug`, `sandbox`, `apply`, `cloud`, `remote-control`, `completion`, `update`, `plugin`, `archive`, `delete`, `unarchive`, `migrate-rollouts` (`help` subcommand list). Skip.
- **TUI**: no subcommand, or `resume`, `fork`, `agents`. `agents` is the multi-thread overview (`help`). Treat it as a viewer, not a session host **[proposal]**.
- Also skip `codex.exe` children used as sandbox helpers (`codex-command-runner`, `codex-windows-sandbox-setup`?) **[VERIFY: Windows sandbox helper image names]**.

### 6.3 Match TUI processes to threads (heuristic; mark the mapping's confidence)

For each TUI process collect: pid, creation time (`GetProcessTimes`), cwd (PEB `CurrentDirectory`, or the `-C/--cd` arg), and argv (`resume <uuid|name>`, `fork <id>`). For each loaded thread with `originator=="codex-tui"` or `source=="cli"` and no `parentThreadId`, collect `cwd`, `createdAt`, `updatedAt`.

1. **Exact**: argv has `resume <uuid>` → that thread, until the thread goes `notLoaded` or another thread in the same cwd starts later than it in that process. The TUI can switch threads with `/new`, `/resume`, `/fork` **[VERIFY: slash command names in 0.163]**.
2. **cwd + time**: threads whose normalized cwd (case-insensitive, `\`/`/`, drive letter) equals the process cwd. Assign one-to-one by the smallest positive `thread.createdAt − process.start`. With one candidate this is reliable. With several TUIs in one folder it is ambiguous.
3. **Tie-break by title**: with Windows Terminal, the tab title is `<spinner> <thread name> | <project>` (§9.2). Match `Thread.name` against UIA tab names. Otherwise mark the mapping `uncertain`.
4. A thread with no matching TUI process (e.g. `codex queue` started it, the desktop app, or a TUI that exited while the thread is still loaded) → show it with `host = Unknown`. Open and Reply then use the daemon only.
5. A TUI process with no thread yet (startup screen, trust dialog) → no session.

The mapping matters only for **Open** (jump to window) and the **keystroke fallback**. Status, approvals and replies in daemon mode go through the thread id and don't need the pid at all. That is why the daemon-first design tolerates a weak mapping.

### 6.4 What to exclude

`originator=="codex_exec"` / `source=="exec"` (scripts), `source` subagent (show as subagents of the parent), `threadSource` `guardianReview`/`memoryConsolidation`/`feature:*` (internal), `ephemeral==true`, and VS Code-extension threads (`source=="vscode"`) unless a TUI process matches **[proposal]** (`protocol/src/protocol.rs:2948-2971`).

---

## 7. Answering approvals from the island (ranked by reliability)

### 7.1 Rank 1 — daemon JSON-RPC (recommended)

Works for every TUI session in daemon mode (the default for non-elevated users). It needs no config change and no trust prompt. The TUI prompt stays usable in parallel, and the first answer wins (§3.6).

Flow:
1. On `thread/status/changed` → `active` (any flags), `thread/resume {threadId, excludeTurns:!previews}`. Pending requests are replayed.
2. On a request (§3.5), build a `Permission` (or a question form) with its JSON-RPC `id`, `threadId`, `turnId`, `itemId`, kind, detail and **the decisions actually offered**: `availableDecisions` when present, else the defaults below.
3. Buttons → result:

| Card | Allow | Allow for session | Always (prefix/host) | Deny (turn continues) | Stop |
|---|---|---|---|---|---|
| Command | `{"decision":"accept"}` | `{"decision":"acceptForSession"}` | `{"decision":{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":[…proposedExecpolicyAmendment…]}}}` (only if proposed) | `{"decision":"decline"}` | `{"decision":"cancel"}` |
| Network (command with `networkApprovalContext`) | `accept` | `acceptForSession` | `applyNetworkPolicyAmendment` (only if proposed) | `decline` | `cancel` |
| File change | `accept` | `acceptForSession` | — | `decline` | `cancel` |
| Permissions | `{"permissions":<requested>,"scope":"turn"}` | `…,"scope":"session"` | — | `{"permissions":{}}` [VERIFY] | — |
| Question (`requestUserInput`) | `{"answers":{"<qid>":{"answers":["<label>"]}}}` | | | | |
| MCP elicitation (`form`) | `{"action":"accept","content":{…}}` | | | `{"action":"decline","content":null}` | `{"action":"cancel","content":null}` |

   The island's "Deny" maps to **`decline`**, not `cancel`, so a deny doesn't kill the turn (the TUI's own `n`/Esc is `cancel`; §7.3).
4. Keep the Mac safety rules (02-sessions §2.8 "Decide flow"): a **0.8 s settle guard** (the card must have been on screen ≥ 0.8 s and still be the one shown), answer only if we still hold the request, drop the card on `serverRequest/resolved` or `turn/completed`.
5. If the socket drops mid-request, do nothing. The TUI still has the prompt. Reconnect with backoff and re-`resume`.

Risks: the protocol is marked experimental and churns between releases (methods gated by `experimentalApi`). Parse leniently, feature-detect with `-32601`/"unknown variant" (as `AgentCodexServer.failure` does, `mac` `AgentCodexServer.swift:170-177`), and fall back to read-only.

### 7.2 Rank 2 — `PermissionRequest` hook (opt-in, embedded-mode users)

Use it only when the user turns on "Answer Codex approvals from the island even without the background server", and **don't install it when rank 1 is available**: in daemon mode it would block the request from reaching the TUI, and us, until it returns.

Design, if built: the hook binary is the existing `vorssaint-hook.exe` with `--codex`. It reuses the Claude pipe transport (02-sessions §6.2.4). The app answers within a **short window** (proposal: 30 s, or until the island is dismissed). Otherwise it prints nothing and exits 0, so the TUI prompt appears. Map the island buttons to `allow` / `deny` only; there is no "for session" or "stop". Trust: after installing, tell the user to accept it in Codex's "Hooks need review" screen.

### 7.3 Rank 3 — keystrokes into the terminal (last resort, explicit click only)

Default approval keys (`tui/src/keymap.rs:1947-1953`; overlay `tui/src/bottom_pane/approval_overlay.rs`):

| Prompt (title) | Approve | For session | Prefix/host | Deny, continue | Abort turn |
|---|---|---|---|---|---|
| "Would you like to run the following command?" | `y` | `a` (often not offered) | `p` (when proposed) | `d` (only if offered; **not in the default list**) | `n` / Esc |
| `Do you want to approve network access to "<host>"?` | `y` | `a` | `p` | — | `n` / Esc |
| "Would you like to make the following edits?" | `y` | `a` | — | — | `n` / Esc |
| "Would you like to grant these permissions?" | `y` (turn), `r` (strict review) | `a` | — | `d` / Esc (safe deny here) | — |
| "`<server>` needs your approval." (MCP) | `y` | | | `n` | Esc / `c` |

Digits `1`–`9` pick the Nth option and confirm. `Ctrl+C` cancels and clears the whole queue (`list_selection_view.rs:1405-1428`; `approval_overlay.rs:536-537`).

Risks, all from source:
- **No deny-and-continue for commands or edits by default.** The only deny is `n`/Esc = `cancel` = turn aborted (`approval_overlay.rs:828`; `protocol/src/approvals.rs:365-397`).
- **Esc is dangerous everywhere**: in the composer it interrupts the running turn (`keymap.rs:1675`); in `request_user_input` it interrupts the turn (`bottom_pane/request_user_input/mod.rs:1230-1235`).
- **Wrong-prompt risk**: approvals are delayed until 1 s after the last composer keystroke (`bottom_pane/mod.rs:238, 772-816`). Several approvals queue, and the live queue pops **newest first** (`approval_overlay.rs:495-501, 608-614`). A `y` can hit a different request than the one on our card. In embedded mode we can't see which request is on screen.
- Requires focus-free injection (`WriteConsoleInputW` via the existing console helper, `win` `hosts/console.rs`). `SendInput` stays the 02-sessions last resort.

Rule: offer keystroke approval only for **Approve** (`y`), only when we know exactly one request is pending (hook mode, or a title saying Action Required with a single request), and never send Esc/`n`.

---

## 8. Typing a reply

### 8.1 Daemon (recommended)

- Thread `idle` → `turn/start {threadId, input:[{"type":"text","text":"<reply>","text_elements":[]}], clientUserMessageId:"<uuidv7>"}`. Images → add `{"type":"localImage","path":"C:\\…\\x.png"}` items (`v2/turn.rs:443-475`).
- Thread `active` → `turn/steer {threadId, input, expectedTurnId:<current turn id>}` (this matches TUI Enter while working, `tui/src/chatwidget/input_flow.rs:49-84`). Or `thread/queue/add` to deliver after the turn (TUI Tab). Proposal: the island's Send while working = queue, matching the Mac rule "never type while busy" in spirit; steer only as an explicit option.
- **Waiting** (approval or question pending) → refuse, as on the Mac (02-sessions §6.1 reply gate).
- `turn/start` on a thread loaded by the TUI is allowed (`turn_processor.rs:525-540`; `thread_input.rs:23-35`). **[VERIFY]** that the TUI renders a turn started by another client (it is subscribed, so it should get `turn/started` and the user message item).

### 8.2 Keystrokes (embedded mode)

What the TUI does with input on Windows:
- It **disables `ENABLE_VIRTUAL_TERMINAL_INPUT`** and reads console key records (`tui/src/tui.rs:245-251`; `tui/src/tui/windows_console.rs:9-11, 45-66`). Bracketed paste is requested (`EnableBracketedPaste`, `tui.rs:248`), but an injected `ESC [ 2 0 0 ~` arrives as an **Esc key** plus characters **[inference from the console mode; VERIFY]**. **Never use the bracketed-paste wrapper for Codex** (the Claude path in 02-sessions §6.2.2 B uses it).
- Enter submits, Tab queues. Newline keys are Ctrl+J, Shift+Enter (needs keyboard enhancement), Alt+Enter (`keymap.rs:1691-1707`; `chat_composer.rs:3502-3509`).
- Paste-burst heuristic: ≥ 3 chars ≤ 8 ms apart start a burst. On Windows the burst flushes after 60 ms idle, and Enter within 120 ms of a burst char becomes a **newline** (`bottom_pane/paste_burst.rs:155-170`; `chat_composer.rs:3134-3142, 3613-3623`). `tui.disable_paste_burst = true` turns this off, and then every `\r` submits.

Recipe (via `WriteConsoleInputW` `KEY_EVENT` records from the console helper):
1. Refuse if the session may be waiting (title Action Required, hook pending).
2. Send the text as Unicode key events (`VK_PACKET` or `uChar` with VK 0). Send each `\n` as **Ctrl+J** (`wVirtualKeyCode='J'`, `LEFT_CTRL_PRESSED`, `uChar=0x0A`) **[VERIFY: crossterm maps it to Ctrl+J]**. Don't send `\r` inside the text.
3. Wait **≥ 250 ms** (clears the 60 ms flush and the 120 ms Enter window with margin).
4. Send one `VK_RETURN` key down/up.
5. Never send ESC.

VS Code-family terminals: the bridge's existing `send` command wraps text in bracketed paste. **Don't use it for Codex.** Use the existing generic `steps` command (`[{"text":"…"},{"wait":300},{"enter":true}]`, 02-sessions §6.1). Line breaks: `sendText` goes through ConPTY's VT input parser, which turns LF into a key event **[VERIFY: LF → Ctrl+J, ESC sequences → Esc key]**.

---

## 9. Jump to window ("Open")

### 9.1 From the pid

When §6.3 found the TUI's `codex.exe` pid, reuse `hosts::locate(pid)` / `hosts::focus` unchanged (`win` `hosts/mod.rs:65-127`). The ancestor chain includes `node.exe` and `cmd.exe` for npm installs, and the classifier already looks for the first terminal/IDE ancestor.

### 9.2 Terminal title (Windows Terminal tab matching, and an embedded-mode waiting signal)

- The TUI writes OSC 0 (`ESC]0;<title>BEL`, ≤ 240 chars) (`tui/src/terminal_title.rs:35, 102-107`). The config is `tui.terminal_title`, default `["activity","thread-name","project-name"]`, joined with ` | ` (`config/src/types.rs:934-939`; `bottom_pane/title_setup.rs:191-201`):
  - working: `⠋ <thread name> | <project>` (braille spinner; thread name omitted while unnamed; project = root name or cwd basename ≤ 24 chars, `chatwidget/status_surfaces.rs:29-44, 567-579`)
  - any approval / question / elicitation: `[ ! ] Action Required | <thread> | <project>`, blinking with `[ . ]` every 1 s (`status_surfaces.rs:386-401`; `bottom_pane/action_required_title.rs`)
  - Only with the `activity` item configured. Users can change or clear the title.
- The TUI also emits `ESC]7501;state=idle|working|blocked:app=codex ESC\` (`tui/src/terminal_program_status.rs:109-115, 234-237`). It is visible only in the output stream, so not useful to us.
- Desktop notifications: with `tui.notification_method = auto`, Windows Terminal and VS Code get a bare BEL, not OSC 9 (`tui/src/notifications/mod.rs:21-64`). Nothing for us to read.

---

## 10. Tokens, cost and model names

- Tokens per session: `tokenUsage.total` from `thread/tokenUsage/updated` (daemon), or `token_usage_record.thread_token_usage` / `usage` from the rollout. Map them exactly like `AgentLogParser.codexTokens` in the Mac app (`Sources/Vorssaint/Services/AgentUsage/AgentLogParser.swift:146-152`), which treats input as *including* cache reads and cache writes: `cache_read = min(input, cached)`, `cache_write = min(input − cache_read, cacheWrite)`, `input = input − cache_read − cache_write`, `output`, `reasoning`. Then Sessions and the AI page agree (03-agents §3.1).
- Cost: the existing price list (`Resources/agent-prices.json`, `updated 2026-10-09`, codex table: 73 OpenAI ids with sources `developers.openai.com/api/docs/pricing`, `…/codex/models`; 03-agents §6). Model matching and the cost formula follow 03-agents §6.3–6.4. Fast tier: `thread_settings.service_tier` (`mac` `CodexReader.swift:41, 98-101`). Examples (USD / 1M tokens): `gpt-5.6-sol` in 4 / out 20 / cache read 0.4 / cache write 5, long context above 272 000 ×2 in / ×1.5 out; `gpt-5.6-luna` 0.2 / 1.2 / 0.02 / 0.25; `gpt-6.1-sol` 2 / 10 / 0.1 / 2.5.
- This is an **API-equivalent cost**. ChatGPT-plan users (`plan_type` `plus` here) don't pay per token. Use the same caption as Claude sessions.
- Model names seen on this Mac: `gpt-5.6-sol` (config default, TUI), `gpt-5.6-luna` (`codex exec`), `gpt-6-astra` (in `[tui.model_availability_nux]`). Display names come from 03-agents §6.5. The live catalog is `codex debug models` (`help`).
- Limits (5 h / weekly windows) stay on the AI page (03-agents §3.1/§3.3). The session page may show the context-window fill from `modelContextWindow` **[proposal]**.

---

## 11. Proposal: Codex sessions in the Rust engine

Keep the 02-sessions split: a pure `engine` with unit tests, and an I/O `service`. Codex becomes a second **source** that feeds the same engine output type.

```rust
// sessions/models.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Provider { Claude, Codex }

pub struct Session {
    pub provider: Provider,          // NEW; UI picks the mark/tint (Codex #7D99FF, 03-agents §10.1)
    pub id: String,                  // NEW convention: "claude:<sessionId>" / "codex:<home-hash>:<threadId>"
    pub native_id: String,           // NEW: sessionId or threadId as the vendor knows it
    pub pid: Option<i32>,            // CHANGED: Codex sessions may have no known TUI pid (§6.3)
    pub host_confidence: HostConfidence, // NEW: Exact | Heuristic | Unknown
    pub reply_route: ReplyRoute,     // NEW, see below (replaces accepts_replies' meaning)
    // …all existing fields unchanged (phase, tools, permission, tokens, cost, branch, model…)
}

pub enum ReplyRoute { None, ClaudeHost, CodexDaemon { home: PathBuf }, Keystrokes }

pub struct Permission {
    // existing fields…
    pub origin: PermissionOrigin,    // NEW: ClaudeHook{request_id} | CodexRpc{home, rpc_id: Value, thread_id, turn_id, item_id} | CodexHook{..}
    pub kind: PermissionKind,        // NEW: Command | Network | FileChange | Permissions | Mcp | Tool
    pub decisions: Vec<Decision>,    // NEW: what may be offered (Allow, AllowSession, AllowRule(Vec<String>), Deny, Stop)
}
```

- `SessionRecord` (Claude registry row) stays Claude-only. Add `sessions/codex/` with:
  - `client.rs`: AF_UNIX connect (`uds_windows` on Windows, `std::os::unix` elsewhere), peer check (§3.1.3), WebSocket handshake (`tungstenite` client on the stream), JSON-RPC with request ids, reconnect with backoff, one client per `CODEX_HOME`.
  - `protocol.rs`: serde types for the subset in §3.3–3.5, all `#[serde(default)]` and lenient (unknown variants → `Other`).
  - `rollout.rs`: the embedded-mode tailer (shares cursor logic with the usage reader in 03-agents §7).
  - `mapping.rs`: §6 process classification + thread↔pid matching (pure; unit-tested with synthetic process lists).
  - `source.rs`: turns daemon events / rollout lines into the engine's existing facts: `begin(turn)`, `end(turn, completed)`, `start(tool)`, `finish(tool)`, `permission(..)`, `usage(..)`, plus a Codex-only `status(ThreadStatus, at)` fact equivalent to the Claude registry status (02-sessions §3.2 "Registry ingest").
- Engine rules: reuse §3.2/§3.3 of 02-sessions with the Codex status mapped onto the registry rules (`active` = busy, `idle` = idle, flags = waiting with `waitingFor` = "permission prompt" / "input needed"). Port the phase table in §5.1 as tests.
- Service: `sessions_decide` / `sessions_answer` dispatch on `Permission.origin`. Codex RPC answers are written on the daemon client; the hook path is shared with Claude. `sessions_reply` dispatches on `reply_route`.
- Settings: "Show Codex sessions" (default on when Codex is enabled on the AI page), "Answer Codex approvals from the island" (rank 1, default on), and an advanced, off-by-default "Use a Codex hook when the background server is off" (rank 2).
- Clawd: Codex sessions get the same buddy seeding by native id **[open: Codex-styled costume?]**.

---

## 12. Privacy (same rules as 02-sessions §7)

- With previews off, decode no content: ignore `preview`, `name` (it is user-chosen, but it can still hold text from the conversation; treat it like Claude's auto names), `agentMessage`, `userMessage`, `reasoning`, command output, `last_assistant_message`, notify `input-messages`. Keep ids, times, tool **types**, model, tokens, cost, cwd, branch.
- Always show a pending approval's command/cwd/reason/paths and question texts, then drop them once answered.
- Never read `auth.json`, never log `config.toml`, never send anything over the network. The daemon socket is local and user-only.

---

## Open questions / to verify in the Windows VM

1. **Daemon on Windows, end to end.** Start `codex` in Windows Terminal (non-elevated). Confirm `%USERPROFILE%\.codex\app-server-control\app-server-control.sock` and `app-server-daemon\daemon.pid` appear, and that a third-party Rust exe can AF_UNIX-connect, do the WS handshake to `ws://localhost/rpc`, and `initialize`.
2. **Approve from outside.** With a TUI showing "Would you like to run the following command?", `thread/resume` from our client, receive the replayed `item/commandExecution/requestApproval`, answer `{"decision":"accept"}`, and confirm the TUI overlay closes (`serverRequest/resolved`) and the command runs. Repeat for `decline` (turn continues) and for a file change and `requestUserInput`. Answer in the TUI and confirm we get `serverRequest/resolved`.
3. **`thread/status/changed` without subscription.** Does an unsubscribed client really get it for TUI threads, with `waitingOnApproval` while the overlay is up?
4. **`turn/start` / `turn/steer` / `thread/queue/add` from our client.** Does the TUI show the injected user message and stream the turn? What does the user see?
5. **Side effects of our subscription.** Does a subscribed client keep a thread loaded after its TUI exits (§3.7)? Does our idle connection keep the daemon alive? Does our `clientInfo` overwrite show up anywhere visible (notify `client`, analytics)?
6. **Embedded-mode triggers on Windows.** Confirm the elevated-terminal fallback and the job-object case. What do `daemon.pid` and the writer locks look like then?
7. **Process tree.** For npm (`codex.cmd`), standalone (`%LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe`) and winget installs, record the full ancestor chain under WT, VS Code and conhost. Is the daemon's parent ever the TUI (it is spawned detached)? Which helper exes (sandbox) also show up as `codex.exe`?
8. **pid ↔ thread mapping** (§6.3): two TUIs in the same folder, `/new` and `/resume` inside a TUI, `codex resume --last`. Measure how often cwd+time is ambiguous. Check that the WT tab title (UIA `TabItem.Name`) carries `Thread.name`, and how it looks before a thread has a name.
9. **Keystroke fallback in ConPTY.** Confirm that `WriteConsoleInputW` chars, Ctrl+J and a delayed `VK_RETURN` submit a multi-line reply. Confirm that an injected `ESC[200~` is seen as Esc (do this on an idle session: Esc on an empty composer mid-turn interrupts). Confirm `y` approves. Do the same through VS Code `terminal.sendText` (LF and ESC handling).
10. **Hooks on Windows** (only if rank 2 is built): which shell runs `commandWindows` (PowerShell vs cmd), the quoting of a path with spaces, the "Hooks need review" flow, and that a hook returning empty stdout quickly lets the TUI prompt appear.
11. **Permissions deny wire format** `{"permissions":{}}` and `turn/interrupt` params (§3.3, §3.5).
12. **`thread/read` last-turn status** with `includeTurns:false` (§5.1).
13. **Protocol stability**: diff `codex app-server generate-json-schema` between 0.163 and the next release, and decide on a minimum supported version (show read-only below it).
14. **Codex desktop app / VS Code extension on Windows**: do they share `%USERPROFILE%\.codex` and the same daemon? If so their threads appear in `thread/loaded/list` (`source` `vscode`/`custom`), and we need the exclusion in §6.4 or a read-only row.
15. **Long `CODEX_HOME` paths** vs the 108-byte AF_UNIX limit on Windows.
