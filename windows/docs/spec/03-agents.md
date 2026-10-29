# 03 — AI Agents section (usage, limits, API value): behavior and data-source spec

This spec covers the macOS "AI Agents" island page in enough detail to rebuild it on Windows (Tauri 2, Rust backend, React UI) without reading Swift. The Claude Code **sessions** section is covered in `02-sessions.md`. The two sections share `~/.claude/sessions/<pid>.json`, but the agents section reads only `pid`, `sessionId` and `pidDomain` from it (§2.1.4).

Path abbreviations used in `file:line` refs:

| Prefix | Path |
|---|---|
| `AU/` | `Sources/Vorssaint/Services/AgentUsage/` |
| `AUH/` | `Sources/Vorssaint/Services/AgentUsage/Harness/` |
| `NAS` | `Sources/Vorssaint/Services/Notch/NotchAgentSupport.swift` |
| `NAV` | `Sources/Vorssaint/UI/Notch/NotchAgentsView.swift` |
| `NAC` | `Sources/Vorssaint/UI/Notch/NotchAgentComponents.swift` |
| `NASt` | `Sources/Vorssaint/UI/Notch/NotchAgentStrip.swift` |
| `NAAV` | `Sources/Vorssaint/UI/Notch/NotchAgentAnimationView.swift` |
| `NASet` | `Sources/Vorssaint/UI/Settings/NotchAgentsSettings.swift` |
| `CAS` | `Sources/Vorssaint/UI/Settings/CodexAccountsSettings.swift` |
| `STR` | `Sources/Vorssaint/Core/NotchAgentStrings.swift` (English at :191–288) |
| `CAStr` | `Sources/Vorssaint/Core/CodexAccountStrings.swift` (English at :33–51) |
| `DEF` | `Sources/Vorssaint/Core/Defaults.swift` |
| `NS` | `Sources/Vorssaint/Services/Notch/NotchService.swift` |
| `PRICES` | `Resources/agent-prices.json` |

---

## 0. Executive summary

* There are four providers: `claude`, `codex`, `opencode` and `copilot` (`AU/AgentUsageModels.swift:8-33`). Their display names are "Claude", "Codex", "OpenCode" and "GitHub Copilot". Copilot never reports plan limits (`reportsLimits`, `:32`).
* **Everything is local, read-only file parsing**, with three exceptions:
  1. **Claude account limits.** The app spawns the user's own `claude` CLI in headless stream-json mode and sends a `get_usage` control request (§2.2). The app never touches tokens or the Keychain.
  2. **Codex account limits and banked resets.** The app spawns `codex app-server` and speaks JSON-RPC: `account/read`, `account/rateLimits/read` and `account/rateLimitResetCredit/consume` (§3.3). Again no credential is read by the app.
  3. **Price list.** The app does one HTTPS GET per day of a JSON file on GitHub (§6).
* **The Mac build reads no credentials and makes no Keychain calls** in this section. All auth is delegated to the vendor CLIs, which find their own sign-in. On Windows that means **Windows Credential Manager does not need to be touched**: `claude.exe` and `codex` resolve their own credentials. On Windows Claude Code keeps OAuth credentials in `%USERPROFILE%\.claude\.credentials.json` (needs verification), and Codex keeps them in `%USERPROFILE%\.codex\auth.json` or the OS keyring, depending on its `cli_auth_credentials_store` config (needs verification).
* **Costs are "API value".** Tokens are priced at public API list prices. This is not what the user pays on a plan (`STR` `valueNote`).
* History covers 91 days (13 weeks) (`AU/AgentUsageSummary.swift:85`). Counters persist in a binary archive so a relaunch only reads newly appended bytes (§7).

---

## 1. Lifecycle, timers and cadence (service level)

The main service is `AgentUsageService` (`AU/AgentUsageService.swift`). It is a singleton. It reads on a private utility queue, publishes `snapshot: AgentUsageSnapshot` to the UI, and emits an `events` stream of `AgentUsageEvent` (`AU/AgentUsageModels.swift:156-161`).

### 1.1 On/off gating
* The section is enabled when the island is enabled, the feature is available, `notchAgentsEnabled == true`, and the island's module list contains `.agents` (`NAS:75-84`).
* The per-provider toggles `notchAgentsClaude`, `notchAgentsCodex`, `notchAgentsOpenCode` and `notchAgentsCopilot` all default to `true` (`NAS:86-99`). Settings refuses to switch off the last remaining provider (`NASet:247-248`).
* `syncWithPreferences()` (`AU/AgentUsageService.swift:120-140`):
  * When the enabled provider set changes, the service does a full **stop without keeping progress** and then restarts. A provider that is newly on is read from the start of its logs.
* `pause()` (`:145-161`) is called while the island is away (display asleep, Mac locked; `NotchLockScreenService.swift:176,184`, `NS:1322`).
  * It cancels the in-flight Claude check, stops the timer, poller and watcher, and saves progress.
  * It **keeps everything in memory**.
* `resume()` (`:164-175`) re-arms the watcher, does a full rescan (`filesChanged([], rescan: true)`), restarts polling and republishes.
* `stop(keepingProgress:)` (`:177-226`) clears everything.
  * If the section was switched off, it **deletes the archive file** (`settleArchive(keeping:false)` → `AgentUsageArchive.remove()`, `:322-326`).

### 1.2 Timers

| What | Value | Ref |
|---|---|---|
| Main tick (main thread, tolerance 10 s) | 30 s | `AU/AgentUsageService.swift:35,261-266` |
| Fast poll of "open" logs (logs modified < 30 min ago, or logs holding a turn/waiting turn) | 2 s, leeway 500 ms; armed only while wanted | `:39-40,359-401` |
| Publish coalescing (bursts → 1 publish) | 1 s | `:42,548-555` |
| Archive save while running | every 5 min, plus on pause/stop | `:45,587` |
| Re-discover log roots (e.g. first Codex session appears) | every 300 s inside the tick | `:573-582` |
| Day-old logs polled by size/inode | each 30 s tick (`pollOpenLogs(within: 86_400)`) | `:570` |
| File-system events | FSEvents, file-level, 1.0 s latency, `WatchRoot` | `AU/AgentUsageStore.swift:868-930` |
| Turn idle → "waiting" | 10 min (`NotchAgentSupport.idleTurn`) | `NAS:73`, `AU/AgentUsageStore.swift:296-303` |
| Waiting turn dropped | Claude 1 h, others 6 h | `AU/AgentUsageStore.swift:44-46` |
| "Late end" (a finish older than this is not a notice) | 5 min | `:36` |
| Offline grace before Claude turns are closed | 20 s of awake time offline | `:39,355-368` |
| OpenCode settle (a step ended expecting more, then nothing follows) | 30 s | `AU/AgentLogParser.swift:164`, `AU/AgentUsageStore.swift:310-318` |
| Claude account check | 60 s while the page is visible; 5 min background; failures back off (§2.2) | `AU/AgentClaudeServer.swift:19-47` |
| Codex resets check | at most every 5 min; 60 s after a failure | `AU/AgentCodexResetService.swift:27-31` |
| Codex profile accounts | 5 min; 60 s after a window renewed; 5 min after a failure; 30 min for "settled" failures; page timer 30 s | `AU/CodexAccountLimits.swift:105-119`, `AU/CodexProfileLimitsService.swift:210` |
| Price list download | once per 24 h; 6 h after a failure | `AU/AgentPriceSource.swift:12-14` |
| UI clock: page grid | `TimelineView` every 15 s | `NAV:61` |
| UI clock: "Now" card rows | 1 s | `NAV:575` |
| UI clock: strip (elapsed/limit readouts) | 1 s; none for tokens/cost readouts | `NASt:105-118`, `NAS:39-41` |
| UI clock: resting wing | 60 s | `NASt:130` |

**Windows equivalents**
* Watcher: use `ReadDirectoryChangesW` (the Rust `notify` crate) with `FILE_NOTIFY_CHANGE_SIZE|LAST_WRITE|FILE_NAME`, recursive.
  * NTFS updates a directory entry's size **lazily for files held open by a writer**, so change notifications for appends can be late or coalesced. The 2 s stat-poll of "recent" logs is therefore **mandatory**, not optional. It is the same reason the Mac polls: "File events report a written file only once it closes" (`AU/AgentUsageService.swift:36-38`).
  * Poll with `GetFileInformationByHandle` (size + `nFileIndexHigh/Low` as inode) or `std::fs::metadata` (needs verification for open files).
* Pause/resume: hook `WM_WTSSESSION_CHANGE` (lock/unlock) and `WM_POWERBROADCAST` / `GUID_CONSOLE_DISPLAY_STATE` (display off).
* Network: Mac uses `NWPathMonitor` (`:440-459`). On Windows use `INetworkListManager` / `NetworkInformation.NetworkStatusChanged`, or the `netstat2`/`if-watch` crates.

### 1.3 Initial read (`start`, `AU/AgentUsageService.swift:268-316`)
1. Load prices first: the newer of the bundled list and the cached list (§6).
2. Discover log files modified within the 91-day horizon (§2–§5 roots).
3. If the archive exists **and was saved for the same provider set**, resume store and cursors from it (§7).
4. Read each file from its cursor. The read is cancellable.
5. `closeIdleTurns`, then `closeEndedTurns(atLaunch:true)` using the Claude session registry (§2.1.4).
6. Read the Claude plan (`~/.claude.json`) and the Claude-app history file.
7. Set `reportsTransitions = true`, so history never replays as notices.
8. If today's API value is already ≥ the daily budget, mark the budget as already announced.
9. Start the watcher, polling and network monitor. Publish, then save.

---

## 2. Claude Code

### 2.1 Files read

| # | Mac path | Windows equivalent | Used for |
|---|---|---|---|
| 1 | `~/.claude/projects/**/*.jsonl` | `%USERPROFILE%\.claude\projects\**\*.jsonl` (honour `CLAUDE_CONFIG_DIR` if set — Mac does not, see risks) | usage records, turns |
| 1b | `~/.config/claude/projects/**/*.jsonl` | `%USERPROFILE%\.config\claude\projects` (XDG fallback; unlikely on Windows) | same |
| 2 | `<session>/subagents/*.jsonl` beside `<session>.jsonl` | same structure | subagent usage counted toward parent's turn |
| 3 | `~/.claude/sessions/<pid>.json` (sibling `sessions` of each `projects` root) | `%USERPROFILE%\.claude\sessions\<pid>.json` | ends turns of killed sessions |
| 4 | `~/.claude.json` | `%USERPROFILE%\.claude.json` | plan name, organization UUID, `cachedUsageUtilization.fetchedAtMs` |
| 5 | `~/.claude/settings.json` (`env` block only) | `%USERPROFILE%\.claude\settings.json` | env opt-outs and proxy passed to the CLI check |
| 6 | `~/Library/Application Support/Claude/plan-usage-history.json` | **probably** `%APPDATA%\Claude\plan-usage-history.json` — **unverified**, the Windows Claude app may not write it at all | plan-limit percentages fallback |

Roots: `AU/AgentUsageStore.swift:475-481`. Each path is canonicalized with `realpath` so FSEvents paths match (`:485-489`).

Accepted files are any path that ends in `.jsonl` under the root (`:497-505`). The walk is recursive, skipping package descendants (`:761-766`). Ordering is oldest-modified first, and Claude subagent files sort after all sessions, so a parent turn exists before its subagent's usage is applied (`:767`). Subagent→parent mapping: the path contains `/subagents/` → parent = prefix + `.jsonl` (`:680-683`).

#### 2.1.1 Transcript line parsing (`AUH/ClaudeReader.swift`)
Lines are byte-prefiltered before JSON decoding, for speed (`AU/AgentLogParser.swift:98-103`). A line longer than 32 MiB is skipped. Reads happen in chunks of 4 MiB (`AU/AgentUsageStore.swift:687-690`).

**Assistant line** (`"type":"assistant"`, `AUH/ClaudeReader.swift:69-118`):
```json
{
  "type": "assistant",
  "sessionId": "…", "cwd": "/path/to/project", "timestamp": "2026-09-21T23:42:45.078Z",
  "requestId": "req_…", "isSidechain": false, "isApiErrorMessage": false,
  "message": {
    "id": "msg_…", "model": "claude-opus-5-5",
    "stop_reason": "end_turn" | "tool_use" | "stop_sequence" | "max_tokens" | "refusal" | null,
    "content": [ { "type": "tool_use", "name": "Bash", "id": "toolu_…" }, … ],
    "usage": {
      "input_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0,
      "output_tokens": 0,
      "output_tokens_details": { "thinking_tokens": 0 },
      "cache_creation": { "ephemeral_1h_input_tokens": 0 },
      "speed": "fast",            // → fast tier multiplier
      "inference_geo": "us",      // → US-only multiplier
      "server_tool_use": { "web_search_requests": 0 }
    }
  }
}
```
* A usage record is produced only when `message.usage` exists and the model is non-empty and does not start with `<` (`<synthetic>` and the like). Dedup key: `claude:<message.id>:<requestId>`, or `claude:<session>:<epoch>` when both are missing (`:89-92`).
* Streamed duplicates (one line per content block) are merged by taking the **per-field max** (`AU/AgentUsageModels.swift:59-63`, `AU/AgentUsageStore.swift:167-243`).
* Turn end happens when `stop_reason ∈ {end_turn, stop_sequence, max_tokens, refusal}` and `isSidechain != true`. It counts as *completed* unless `isApiErrorMessage == true` or the model starts with `<` (`:100-106`).
* Any other stop reason means `turnBegan` or `turnActive`. `Bash` tool_use ids are remembered as "running commands", which keep the turn alive while offline (`:107-115`).
* `sessionId` sets the session. `cwd` sets the project: the last path component, with anything after `/.claude/worktrees/` stripped (`AU/AgentLogParser.swift:238-243`). **On Windows, split on both `\` and `/`** and strip the `\.claude\worktrees\` equivalent.

**User line** (`"type":"user"`, `:12-67`):
* A text block beginning with `[Request interrupted by user` or `<local-command-std` closes the turn as *not completed*.
* `"toolEndsTurn":true` (non-sidechain) closes it as *completed*.
* While the turn is open, a `tool_result` with `tool_use_id` clears that running command.
* When the turn is closed, a user prompt opens a turn **unless** `isMeta`, `isSidechain`, `isCompactSummary`, or the text starts with `<command-name>` (slash command).

#### 2.1.2 Plan (`~/.claude.json`, `AU/AgentUsageService.swift:701-725`)
This file is re-read only when its mtime changes. Only `oauthAccount` is used:
```json
{ "oauthAccount": {
    "organizationUuid": "…",
    "organizationType": "claude_max",                    // e.g. claude_pro, claude_max, claude_team, claude_enterprise
    "organizationRateLimitTier": "default_claude_max_20x",
    "userRateLimitTier": "…" },
  "cachedUsageUtilization": { "accountUuid": "…", "fetchedAtMs": 1790000000000, "utilization": {…} } }
```
* Plan matching: `lower(rateLimitTier) + " " + lower(organizationType)` is matched by **substring**, in list order, against `PRICES.claude.plans[].match` (`AU/AgentPricing.swift:304-315`).
* With no match, the plan name is `organizationType` with `claude_` removed, `_` turned into a space, and the result capitalized.
* A changed `organizationUuid` clears the Claude limits. A missing `oauthAccount` clears the plan and the limits.

#### 2.1.3 Claude-app plan-usage history (fallback limits, `AU/AgentClaudeAppUsage.swift`)
This file (≤ 4 MiB) is written by the **Claude desktop app** while its menu-bar icon is on.
```json
{ "version": 2,
  "samples": [ { "t": 1790000000000, "org": "<organizationUuid>", "u": { "fh": 12, "sd": 74, "so": 30, "sn": 5 } } ] }
```
* In `version: 1` the window keys sit directly in the sample, beside `t` (`:51`).
* Key meanings (`:36-38`):

  | Key | Window | Kind | Length | Scope |
  |---|---|---|---|---|
  | `fh` | 5-hour session | session | 300 min | — |
  | `sd` | weekly | weekly | 10080 min | — |
  | `so` | weekly | weekly | 10080 min | Opus |
  | `sn` | weekly | weekly | 10080 min | Sonnet |

* Samples are filtered to `org == nil || org == claude.json organizationUuid` (`:112-114`).
* The latest sample must not be more than 5 min in the future, and must be less than 7 days old (`:72-73`).
* Reset-time inference (the file holds percentages only):
  * **Session.** The run of non-zero `fh` readings ending at the latest one is found. Its first reading, or Claude Code's own first request inside the current 5-hour block (§2.3), places the session start, and reset = start + 5 h. If the gap before the run is longer than 30 min, the card shows the midpoint as an *estimate* (`≈`). The latest-possible end is still used for alerts (`:116-150`).
  * **Weekly.** A "drop" is a reading more than 1 point lower than the one before. A drop confirmed by another drop a whole number of weeks away sets the schedule. Renewals land on the UTC hour. A renewal that clearly passed without a drop invalidates the schedule (`:152-239`).
  * A window whose computed reset ≤ now is dropped. A window with no reset and a sample ≥ 24 h old is dropped (`:88-89`).
* "Fresh" means < 30 min old (`:17`). Older readings render at 60 % opacity on the card (`NAS:321-326`).

**Windows:** the Claude desktop app's data dir on Windows is `%APPDATA%\Claude\`. **Whether the Windows app writes `plan-usage-history.json` is unverified.** It may depend on the tray icon, the equivalent of the macOS menu-bar icon. Treat this source as optional.

#### 2.1.4 Session registry (turn closing only, `AU/AgentUsageStore.swift:511-549`)
* For each `sessions` folder next to a Claude log root, the app reads `<digits>.json` (each < 64 KiB) and uses `pid`, `sessionId` and `pidDomain`.
  * A record with `pidDomain != "darwin"` is skipped: it belongs to a container or VM sharing the folder.
  * Liveness check: `kill(pid,0) == 0 || errno == EPERM`.
* `running` holds session ids whose pid is alive. `ended` holds those whose pid is dead (records left behind by a killed process).
* An unreadable record sets `complete = false`, so absence proves nothing.
* A Claude turn whose log file name (= session id) is in `ended` is closed silently. So is one that was `running` before and whose record is now gone (`:328-342`).
* **Windows:**
  * `pidDomain` value on Windows is **unknown**, probably `"win32"`; verify. Accept the local platform's value.
  * Liveness: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `GetExitCodeProcess == STILL_ACTIVE`.
  * Guard against PID reuse by comparing `procStart`/`startedAt` from the record with the process creation time (`GetProcessTimes`). The Mac does not do this; it is recommended.

### 2.2 Account limits via the Claude CLI (the "account check") (`AU/AgentClaudeServer.swift`)
This is the authoritative source. It works like `/usage` and includes use on other machines.

**Executable discovery** (`:53-106`): the first existing executable from
1. `~/.local/bin/claude`, `~/.claude/local/claude`, `/opt/homebrew/bin/claude`, `/usr/local/bin/claude`
2. each absolute entry of the app's `PATH`
3. Node tool folders: `~/.volta/bin`, fnm (`…/aliases/default/bin`, `…/node-versions/<newest>/installation/bin`), `~/.nvm/versions/node/<newest>/bin`, asdf, mise, `~/.bun/bin`, `~/Library/pnpm`, `~/.local/share/pnpm`, `~/.npm-global/bin`

The Claude desktop app's bundled copy is deliberately excluded. The child's `PATH` = the app's PATH + the tool folders (`:96-98`).

**Windows candidates (needs verification):**
* `%USERPROFILE%\.local\bin\claude.exe` (native installer)
* `%APPDATA%\npm\claude.cmd` (npm global)
* `%USERPROFILE%\.bun\bin\claude.exe`
* `%LOCALAPPDATA%\pnpm\claude.cmd`
* Volta `%LOCALAPPDATA%\Volta\bin`
* nvm-windows `%APPDATA%\nvm\<ver>`
* `where claude` on PATH

`.cmd` shims must be launched through `cmd.exe /d /s /c`, or the app must resolve the shim's target `node.exe` + `cli.js`. Spawn with `CREATE_NO_WINDOW`.

**Environment** (`:128-147`):
* Inherit the app's environment, remove `CLAUDECODE`, set `DISABLE_AUTOUPDATER=1`.
* Copy from `~/.claude/settings.json` → `env` the keys `DISABLE_TELEMETRY`, `DISABLE_ERROR_REPORTING`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`, `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY` and `NODE_EXTRA_CA_CERTS`. Booleans become `"true"`/`"false"` and numbers become strings.
* The working dir is `/` (`AU/AgentCLIConversation.swift:36`). On Windows use `%SystemDrive%\` or `%TEMP%`. stderr goes to null.

**Command line** (`:208-210`):
```
claude --print --input-format stream-json --output-format stream-json --verbose \
  --no-session-persistence --tools "" --strict-mcp-config --mcp-config '{"mcpServers":{}}' \
  --setting-sources "" --settings '{"disableAllHooks":true}'
```
The transport is newline-delimited JSON over stdin/stdout. A single overall deadline of **25 s** covers all answers, and output above 4 MiB aborts (`AU/AgentCLIConversation.swift`). End = close stdin; the process is terminated after 2 s and killed 1 s after that (`:57-81`).

**Protocol:**
```json
→ {"type":"control_request","request_id":"initialize","request":{"subtype":"initialize"}}
← {"type":"control_response","response":{"request_id":"initialize","subtype":"success","response":{…}}}
→ {"type":"control_request","request_id":"usage","request":{"subtype":"get_usage","skip_behaviors":true}}
← {"type":"control_response","response":{"request_id":"usage","subtype":"success","response":{
      "rate_limits_available": true,
      "rate_limits": {
        "five_hour":        {"utilization": 42.0, "resets_at": "2026-10-24T18:00:00Z"},
        "seven_day":        {"utilization": 61.0, "resets_at": "…"},
        "seven_day_opus":   {"utilization": 10.0, "resets_at": null},
        "seven_day_sonnet": {"utilization": 3.0,  "resets_at": "…"} } }}}
```
* Other lines are ignored until a `control_response` with a matching `request_id` arrives (`:249-265`).
* An error has `subtype != "success"` and `response.error` as a string. Mapping:
  * contains "supported" or "unknown" → `outdated`
  * contains "auth", "login" or "sign in" → `needsSignIn`
  * otherwise → `unreachable`
* `rate_limits_available != true` → `needsSignIn` (`:230`).
* Window ids: `five_hour`→`claude.fh` (session, 300), `seven_day`→`claude.sd`, `seven_day_opus`→`claude.so` (scope Opus), `seven_day_sonnet`→`claude.sn` (scope Sonnet). A window whose `resets_at` is ≤ the observation time is dropped (`:149-172`).
* **Staleness guard** (`:233-245`). After the answer, the app ends the CLI, waits ≤ 3 s for exit, and re-reads `~/.claude.json`:
  * The org UUID must be unchanged.
  * If `cachedUsageUtilization.fetchedAtMs` is more than 60 s **before the check started**, the CLI answered from a cache, which counts as `unreachable`.
  * `observedAt = min(now, fetchedAtMs)`.
* **Expiry** (`:178-186`). A window disappears at its `resets_at`. A window without one disappears after its own length for a session, or after 24 h otherwise. The app never invents 0 % after a reset.
* **Scheduling** (`AU/AgentUsageService.swift:765-834`):
  * A check runs only when running, not paused, online, Claude is enabled, nothing is in flight, and **limits are in use**. Limits are in use when any of these holds: limit alerts on, the strip readout is "limit", the resting island shows agents, or the page is visible with the Limits card shown (`NAS:205-210`).
  * Interval: no failure → 60 s with the page visible, otherwise 300 s. `unreachable` (n-th in a row) → 5 min × 2^min(3, n−1), capped at 30 min, which gives 5, 10, 20, 30 min. `missing`/`needsSignIn`/`outdated` → 30 min. Coming back online resets the unreachable counter.
* **Source merge** (`:750-760`). While the account reading is younger than 30 min it wins outright. Otherwise the newer of the account reading and the Claude-app reading is used.

### 2.3 Claude 5-hour block estimate (no limits known)
`AgentUsageSummary.currentBlock` (`AU/AgentUsageSummary.swift:234-249`) walks the last 24 h of Claude records in time order. A block starts at the first record after the previous block ended and lasts 5 h; it is current if `end > now`. The Limits card shows it as "Session ⟳ countdown · $API value" with a dimmed time-elapsed meter, and the tooltip "Estimated from this Mac" (`NAV:358-379`).

---

## 3. Codex

### 3.1 Files read

| Mac path | Windows equivalent | Notes |
|---|---|---|
| `~/.codex/sessions/**/*.jsonl` | `%USERPROFILE%\.codex\sessions\YYYY\MM\DD\rollout-*.jsonl` (honour `CODEX_HOME` — Mac reads only `~/.codex`) | rollout logs |
| `~/.codex/archived_sessions/**/*.jsonl` | `%USERPROFILE%\.codex\archived_sessions` | same parser |

Roots: `AU/AgentUsageStore.swift:477`. The parser is `AUH/CodexReader.swift`. Each line is first prefiltered by its top-level `"type"`, found within the first 64 bytes (`AU/AgentLogParser.swift:110-127`).

Line types used (shapes observed on this Mac, Codex CLI 2026-09):
```json
{"timestamp":"…","type":"session_meta","payload":{
   "id":"<thread id>","session_id":"<root thread>","parent_thread_id":"…","cwd":"/path",
   "source": "cli" | {"subagent":{"thread_spawn":{…}}} | {"internal":…} , … }}
{"timestamp":"…","type":"turn_context","payload":{"model":"gpt-5.6","cwd":"/path","service_tier":"fast"|"priority"|…, …}}
{"timestamp":"…","type":"token_usage_record","payload":{"session_id":"…","response_id":"resp_…",
   "usage":{"input_tokens":23507,"cached_input_tokens":21888,"cache_write_input_tokens":0,
            "output_tokens":36,"reasoning_output_tokens":0,"total_tokens":23543}, …}}
{"timestamp":"…","type":"event_msg","payload":{"type":"token_count",
   "info":{"total_token_usage":{…same keys…},"last_token_usage":{…},"model_context_window":…},
   "rate_limits":{"limit_id":"codex","primary":{"used_percent":9.0,"window_minutes":300,"resets_at":1789778369},
                  "secondary":{"used_percent":6.0,"window_minutes":10080,"resets_at":1789845426},
                  "individual_limit":null,"plan_type":"plus", …}}}
{"timestamp":"…","type":"event_msg","payload":{"type":"task_started","started_at":1789770000}}
{"timestamp":"…","type":"event_msg","payload":{"type":"task_complete","completed_at":…,"duration_ms":…}}
{"timestamp":"…","type":"event_msg","payload":{"type":"turn_aborted", …}}
{"timestamp":"…","type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"service_tier":"fast"}}}
```
**Rules:**
* **Token normalisation** (`AU/AgentLogParser.swift:146-152`): `cacheRead = min(input, cached_input_tokens)`, `cacheWrite = min(input−cacheRead, cache_write_input_tokens)`, `input = input − cacheRead − cacheWrite`. Reasoning is reported but is already contained in output.
* **Usage source.** Prefer `token_usage_record`, deduplicated by `codex:<response_id>`. Until the first record is seen, fall back to `token_count.info`: the delta is `last_token_usage`, or `total − previousTotal`, and the key is `codex:<totalsSession>:total:<total.total>` (`AUH/CodexReader.swift:46-97`).
* **Fast tier.** `service_tier ∈ {fast, priority}` applies `fastMultiplier` (`AU/AgentLogParser.swift:155-157`).
* **Subagent.** A thread whose first `session_meta` has `source.subagent` or `source.internal` owns no turn. Its usage counts toward the turn of the log whose `state.session == session_id || parent_thread_id`, picking the most recently modified such log (`AUH/CodexReader.swift:32-40`, `AU/AgentUsageStore.swift:589-598`). Only the **first** `session_meta` in a file describes it; later ones are copied history.
* **Turns.** `task_started` opens a turn and `task_complete` or `turn_aborted` closes it. Only `task_complete` counts as completed; the duration comes from `duration_ms`.
* **Limits from logs** (`AU/AgentLogParser.swift:188-232`):
  * Only the main bucket is kept: `limit_id` empty or `"codex"`. Model-specific buckets are ignored.
  * Windows come from the `primary`/`secondary` slots, but are **identified by length, not slot**. The id is `codex.<window_minutes>`, falling back to `codex.primary`/`codex.secondary`.
  * Kind: ≤ 720 min is a session, 8640–11520 min is weekly, anything else is other.
  * Reset comes from `resets_at` (Unix seconds, or ms if > 1e11), else from `observed + resets_in_seconds`.
  * When both slots are empty, `individual_limit.remaining_percent` becomes `codex.individual` (used = 100 − remaining).
* **Plan** from `rate_limits.plan_type`. The newest observation wins (`AU/AgentUsageStore.swift:80-85`). Mapping: exact match against `PRICES.codex.plans[].match`, else the value capitalized (`AU/AgentPricing.swift:317-324`).
* Limits keep the newest observation regardless of source (`AU/AgentUsageStore.swift:282-286`).

### 3.2 Codex CLI discovery and environment (`AU/AgentCodexServer.swift:53-96`)
* Candidates, first executable wins:
  1. Inside the desktop apps `com.openai.codex` and `com.openai.chat`: `Contents/Resources/codex-cli/bin/codex` and `Contents/Resources/codex`
  2. `~/.local/bin/codex`, `/opt/homebrew/bin/codex`, `/usr/local/bin/codex`
  3. Optionally the login shell's PATH. That lookup is run only when the user presses "Retry" after a `missing` failure, because it runs shell startup files (`AU/AgentCodexResetService.swift:35-38,57-82`).
* Child PATH = the executable's dir + (shell PATH or Homebrew dirs) + the app's PATH.
* **Windows:** candidates are `%APPDATA%\npm\codex.cmd` (npm `@openai/codex`), `%USERPROFILE%\.local\bin\codex.exe`, `%LOCALAPPDATA%\Programs\…` (a Windows Codex desktop app, if one exists — **unverified**), and `where codex`. Instead of the login-shell PATH, read the user `Path` from `HKCU\Environment` plus the system Path from `HKLM\…\Session Manager\Environment`.

### 3.3 `codex app-server` JSON-RPC (`AU/AgentCodexServer.swift:100-230`)
* Command: `codex -c features.plugins=false app-server`. The transport is NDJSON over stdio (same transport as §2.2), with a timeout of 20 s for checks and 30 s for redeem.
```json
→ {"id":1,"method":"initialize","params":{"clientInfo":{"name":"vorssaint","title":"Vorssaint","version":"<app ver>"},"capabilities":null}}
← {"id":1,"result":{…}}
→ {"method":"initialized"}
→ {"id":2,"method":"account/read","params":{}}            // profile reader sends {"refreshToken":false}
← {"id":2,"result":{"account":{"type":"chatgpt","planType":"plus", …}}}
→ {"id":3,"method":"account/rateLimits/read","params":{}}
← {"id":3,"result":{
     "rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":9,"windowDurationMins":300,"resetsAt":1789778369},
                                     "secondary":{…},"individualLimit":{"remainingPercent":…,"resetsAt":…}}},
     "rateLimits":{"limitId":"codex", …},                   // fallback when ByLimitId missing
     "rateLimitResetCredits":{"availableCount":2,
        "credits":[{"id":"…","status":"available","expiresAt":1790000000}]}}}
→ {"id":4,"method":"account/rateLimitResetCredit/consume","params":{"idempotencyKey":"<uuid>","creditId":"<id>"}}
← {"id":4,"result":{"outcome":"reset"|"nothingToReset"|"noCredit"|"alreadyRedeemed"}}
```
* Notifications (anything carrying `method`) and responses with other ids are skipped.
* Sign-in: `account.type != "chatgpt"` → `needsSignIn` (API-key or cloud accounts have no plan) (`:137-139`).
* Errors: code −32601, "unknown variant" or "method not found" → `outdated`; "authentication" or "not logged in" → `needsSignIn`; otherwise `refused` (`:180-187`).
* A result **without** the `rateLimitResetCredits` key means a Codex too old for resets → `outdated` (`:143-144`). `availableCount = max(listed.count, availableCount)`. Credits that are not `available` or already expired are dropped. They sort by soonest expiry.
* Account window keys are camelCase: `usedPercent`, `windowDurationMins`, `resetsAt`, `individualLimit`, `remainingPercent` (`:166-169`).

### 3.4 Banked resets card service (`AU/AgentCodexResetService.swift`)
* `refreshIfStale()` runs when the Resets card appears. The freshness window is 5 min after success or a "settled" failure (`needsSignIn`, `outdated`), and 60 s after other failures.
* `redeem()` takes the reset that expires first. It **reuses the pending idempotency key for 10 min** if the previous attempt got no answer, so a retry never spends twice (`:84-105`).
  * Outcome shown: `reset` → "Limits reset" (green check); `nothingToReset` → "Your usage doesn't need a reset yet"; `alreadyRedeemed` → "That reset was already used"; `noCredit` → "No resets available"; no answer → "Couldn't use the reset" (orange) plus a "Use a reset" retry button (`NAV:897-964`).
  * The outcome stays on the card for 60 s (`NAV:855-857`).
* Every successful summary feeds its limits into the provider-wide store as `source: .account` (`:130`, `AU/AgentUsageService.swift:230-238`). After a banked reset, the card therefore updates immediately.
* Confirmation UX: "Use a reset" → the line "Reset the session and weekly limits now?" with "Cancel" + "Reset". Cancel replaces the asking button, so a double click cannot spend a reset. A changed count cancels the confirmation (`NAV:880-896`).
* A reset expiring within 24 h shows its expiry line in orange (`NAV:917`).

### 3.5 CODEX_HOME accounts (multi-account limits)
* Persistence: `notchAgentsCodexProfiles` holds a JSON string of an array (`AU/CodexAccountLimits.swift:8-29`):
  ```json
  [{"id":"<UUID>","name":"Work","directory":"/Users/me/.codex-work"}]
  ```
  At most 8 entries, ≤ 64 KiB. A name is 1–40 non-blank characters. The directory must be absolute. Ids and canonical paths must be unique.
* Validation (`canonicalDirectory`, `:34-55`), after symlinks are resolved:
  * Not `/` and not `$HOME`.
  * A directory owned by the current uid, with no group/other write (`mode & 0o022 == 0`).
  * Either `auth.json` exists as a regular file owned by the uid, with mode `& 0o077 == 0` and link count 1 (no hard or soft link), **or** `config.toml` exists (the sign-in may live in the Keychain).
* "Find local accounts" discovers `~/.codex` and every `~/.codex-<name>` (`:59-77`). The name is `<name>` capitalized; `.codex` gets "Default".
* Check (`CodexAccountUsageReader.read`, `:138-159`): `app-server` is run with an **allow-listed env**: HOME, USER, LOGNAME, TMPDIR, the proxy vars (upper and lower case), SSL_CERT_FILE, SSL_CERT_DIR, NODE_EXTRA_CA_CERTS, DO_NOT_TRACK and DISABLE_TELEMETRY, plus `CODEX_HOME=<dir>` and `LANG=en_US.UTF-8` (`:121-133`). It calls `account/read {"refreshToken":false}` and then `account/rateLimits/read`. Plan comes from `account.planType` ∈ {free, go, plus, pro, team, business, enterprise, edu}.
* Scheduling (`AU/CodexProfileLimitsService.swift`):
  * One home is checked at a time, as a FIFO queue.
  * Checks run only while the AI page is visible (or when forced by the card's ⟳ button or Settings "Check limits"), Codex is enabled and the Limits card is not hidden (`:71-74`).
  * A 30 s page timer re-queues homes that are due.
  * A renamed home keeps its state. Leaving the page drops unforced queued checks.
  * `needsSignIn`/`invalidProfile` clear the reading. Other failures keep the last reading.
  * State is held **in memory only**.
* UI: once any accounts are added, **the provider's single Codex Limits card is replaced by one card per account**, titled "Codex · <name>" with a plan chip and a ⟳ button or a mini spinner (`NAS:266-281`, `NAV:218-256`).
  * Placeholder when there are no windows: "Reading usage…" while checking; otherwise the failure text or "Refresh to check this account's limits."
  * Caption: the failure, or "Updated <relative>". Orange when failed or stale (≥ 10 min; `NAS:260,321-324`).
  * The Resets card then shows a "Codex · Default account" line, and its row height becomes 116 (`NAS:258,305-308`).
* **Windows** (`windows/src-tauri/src/agent_usage/profiles.rs`, `windows/src/settings/AgentsSettings.tsx`):
  * Persistence: `island.agentsCodexProfiles` in `settings.json`, an array of `{id, name, directory}` with the same limits (8 entries, names 1–40 characters, unique ids and folders compared case- and slash-insensitively). The directory must be a drive path (`C:\…`): network shares are dropped when the settings are read, and a mapped drive that resolves to a share fails the check.
  * POSIX owner/mode checks become an ACL check (`GetNamedSecurityInfoW`). The folder's owner SID must be the current user, or Administrators (S-1-5-32-544), which an elevated window's token makes the owner of what it creates (a deviation from the Mac's `uid` check; administrators can change the folder anyway). Every allow ACE that applies to the folder and grants a write right must name the user, SYSTEM, Administrators or OWNER RIGHTS (an allow-list, so `Everyone`, `Users` and `Authenticated Users` are refused). `auth.json` must also refuse read rights to anyone else, must not be a reparse point and must have link count 1 (`GetFileInformationByHandle.nNumberOfLinks`). The drive root and `%USERPROFILE%` itself are refused.
  * Discovery: the effective default home (`%CODEX_HOME%`, else `%USERPROFILE%\.codex`, named "Default"), then `%USERPROFILE%\.codex` and `%USERPROFILE%\.codex-*`; a junction to a home already found is skipped, and a home is named after the folder a link leads to. Only the effective default home is named "Default" (a `.codex` that is not it becomes "Codex").
  * Env allow-list: the Mac list plus `USERPROFILE`, `USERNAME`, `USERDOMAIN`, `HOMEDRIVE`, `HOMEPATH`, `APPDATA`, `LOCALAPPDATA`, `SystemRoot`, `SystemDrive`, `windir`, `ComSpec`, `PATHEXT`, `TEMP`, `TMP`, `ProgramData`, `ProgramFiles`, `ProgramFiles(x86)`, `ProgramW6432`, `CommonProgramFiles`, `PROCESSOR_ARCHITECTURE`, `NUMBER_OF_PROCESSORS` and `OS`, matched case-insensitively, then `PATH` (the CLI search path), `CODEX_HOME` and `LANG`. Everything else (`OPENAI_API_KEY`, `CODEX_*`, …) is cleared.
  * A check that is called off (home removed, section, Codex or the Limits card turned off) stops reading the app-server at once and ends it.
  * Scheduling: the service's 2 s step re-queues due homes while the page shows, in place of the Mac's 30 s page timer.
  * The invalid-folder alert is an inline warning under the buttons. `Vorssaint.exe --agents-probe <file> --limits` also lists every discovered home with its check (`codexHomes`).

---

## 4. OpenCode

| Mac path | Windows equivalent |
|---|---|
| `~/.local/share/opencode/opencode.db` (+ `-wal`) | **Likely** `%USERPROFILE%\.local\share\opencode\opencode.db`. OpenCode uses `xdg-basedir`, which returns `~/.local/share` on every OS. **Verify**; also honour `XDG_DATA_HOME`. |

* Only that one file is accepted under the root, and the folder is never walked (`AU/AgentUsageStore.swift:500,744-750`). The watcher also maps `opencode.db-wal` → `opencode.db` (`AU/AgentUsageService.swift:535`). "Modified" = max(mtime(db), mtime(db-wal)) (`AU/AgentOpenCodeReader.swift:40-46`).
* Access is SQLite read-only (`SQLITE_OPEN_READONLY|NOMUTEX`) with a busy timeout of 2000 ms (`:79-82`).
* Schema detection: if a table `session_v2` exists, the database is OpenCode 2 (`session_message` + `session_v2`); otherwise it is v1 (`message` + `session`) (`:86-88`). `session.parent_id` marks a subagent session when that column exists (`:104`). The `part` table is used when present (`:105`).
* Incremental cursor = last `rowid` read. The last 64 `(rowid,id)` pairs are kept to detect reverts. If none of them still match, the reader restarts and emits `{"type":"reset"}` (`:267-287`). The first read starts at the 91-day horizon: binary search on `time_created` for v1, the `session_message_time_created_idx` index for v2 (`:289-342`). Replies still being written (no `time.completed`) are re-queried by id until they settle, or for at most 24 h (`:163-190`).
* Only these JSON paths are extracted, so prompt and reply text never leaves SQLite (`:32-36`): `$.role` (v2: column `m.type`), `$.parentID`, `$.modelID`, `$.model_id`, `$.model`, `$.tokens`, `$.cost`, `$.finish`, `$.time`, `$.path.cwd`, `$.status`, `$.outcome`. Two flags are also computed: `tool_calls` (an unexecuted tool part exists) and `auto_compaction`. `error` becomes a boolean.
* Row JSON handed to the parser (`:229-249`):
  ```json
  {"role":"assistant","id":"msg_…","session_id":"ses_…","parent_session_id":"","directory":"/proj",
   "path":{"cwd":"/proj"},"parentID":"msg_user…","modelID":"claude-sonnet-5-5",
   "tokens":{"input":0,"output":0,"reasoning":0,"cache":{"read":0,"write":0}},"cost":0.0123,
   "finish":"stop"|"tool-calls"|"unknown"|…,"time":{"created":1790000000000,"completed":…},
   "time_created":…,"time_updated":…,"tool_calls":true,"auto_compaction":true,"error":true,"version":2}
  ```
* Usage (`AUH/OpenCodeReader.swift:266-298`): `output = tokens.output + tokens.reasoning`. Cost uses the list price when the model is known, else `cost` as recorded. A recorded cost > 0 is flagged `reportedCost` and is never repriced. Key: `opencode:<session>:<message id>`.
* Turns:
  * The state machine is kept per session, in the file `opencode.db#<session>`.
  * A turn ends when the reply has an error, a `finish` other than `tool-calls`/`unknown` with no pending tool calls, or completes without a finish.
  * A v2 `idle` row ends the turn; `outcome=="succeeded"` means completed.
  * A step that ends and expects more marks the turn "settled", and the turn closes silently after 30 s.
  * Subagent sessions (non-empty `parent_session_id`) never track a turn; their usage counts toward `db#<parent>` (`AU/AgentUsageService.swift:495-499`).
* **Nothing from OpenCode is archived.** The database is re-read from the horizon on every launch (`AU/AgentUsageService.swift:337-339`, `AU/AgentUsageStore.swift:394`).
* OpenCode has **no limits**. Its Limits card shows "Today", cost (≥/— when unpriced), tokens, cache %, and a relative last-used time (`NAV:383-419`).
* **Windows:** read the database with `rusqlite` (bundled SQLite ≥ 3.38 for `json_extract`/`json_each`). WAL readers need the `-shm` file. Open with `immutable=0` and read-only; this works while OpenCode writes. A read-only open can fail if `-shm` is missing and the directory is not writable (rare).

---

## 5. GitHub Copilot (CLI and app)

| Mac path | Windows equivalent |
|---|---|
| `~/.copilot/session-state/<session>/events.jsonl` | **Likely** `%USERPROFILE%\.copilot\session-state\<session>\events.jsonl`; honour `XDG_STATE_HOME`/`COPILOT_HOME` if present. **Verify.** |

* Only `<root>/<non-hidden dir>/events.jsonl` is accepted, exactly two levels deep (`AU/AgentUsageStore.swift:502-505`). The session folder can contain whole workspace checkouts, so it is **never walked** (`:755-760`).
* The first read streams the whole file and passes only lines whose top-level `type` is one of the event types below (`AU/AgentUsageStore.swift:813-824`). After that, reading is append-only as for the other providers.
* Events (`AUH/CopilotReader.swift`, `AU/AgentLogParser.swift:179-182`); envelope `{"type":…,"id":…,"timestamp":…,"agentId":…,"data":{…}}`:
  * `session.start`: `data.sessionId`, `data.selectedModel`, `data.context.{gitRoot|cwd|repository}` → project.
  * `session.context_changed`: `data.{gitRoot|cwd|repository}`. `session.model_change`: `data.newModel`.
  * `user.message` / `assistant.turn_start`: `data.turnId`, `data.model` → open the turn.
  * `assistant.message`: `data.apiCallId`, `data.messageId`, `data.model`, `data.toolRequests`, `data.phase` (`thinking`/`commentary` is not final).
    * Each message counts as one **request** with zero tokens and cost 0. The key is `copilot:<session>:<root|agent:id>:<api:…|message:…>:activity`.
    * Messages with a non-empty `agentId` are subagents. They count as activity but never change the turn.
  * `assistant.turn_end` (`data.turnId`): the turn closes only if the last root message was a final response.
  * `abort`: the turn closes as not completed.
  * `session.shutdown`: `data.modelMetrics = { "<model>": { "requests":{"count":N}, "tokenDetails":{"input":{"tokenCount":…},"cache_write":{…},"cache_read":{…},"output":{…}}, "usage":{"inputTokens","cacheWriteTokens","cacheReadTokens","outputTokens","reasoningTokens"} } }`.
    * These counters are **cumulative per session**. The app stores growth since the previous shutdown, with request-count reconciliation (`AUH/CopilotReader.swift:118-161`).
    * Records are `isAggregate` and priced at base rates, never long-context.
* Copilot usage never feeds a live turn's tokens or cost; a turn is activity only (`AU/AgentUsageStore.swift:65-68`).
* There are no limits and no plan. The Limits card is not created for Copilot (`reportsLimits == false`).

---

## 6. Pricing (`AU/AgentPricing.swift`, `AU/AgentPriceSource.swift`, `PRICES`)

### 6.1 Sources and precedence
1. **Bundled**: `Resources/agent-prices.json` (15.6 KB; `updated: 2026-10-09`; 25 Claude models, 73 OpenAI models).
2. **Cached download**: `~/Library/Application Support/<bundle id>/agent-prices.json`, written atomically with mode 0600 (`AU/AgentPriceSource.swift:23-39`). On Windows: `%APPDATA%\<app id>\agent-prices.json`.
3. **Remote**: `GET https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/Resources/agent-prices.json` (`:11`).
   * Header `User-Agent: Vorssaint/<version>`. Ephemeral session: no cookies, no cache. Timeouts: 15 s per request, 30 s per resource. Size ≤ 256 KiB. Status must be 200. Redirects are followed only to the same https host (`:53-101`).
   * The download runs only when `notchAgentsPriceUpdates` (default true) is on, at most every 24 h, and 6 h after a failure (`AU/AgentUsageService.swift:855-884`).
   * **The Windows port must decide whether to keep this URL**, because it is the upstream project's repository.
* Precedence: the newer `updated` day wins; on the same day the download wins (`AU/AgentPricing.swift:98-102`). After a new list is installed, all non-reported records are repriced (`AU/AgentUsageStore.swift:263-274`).

### 6.2 Schema (version 1, `AU/AgentPricing.swift:80-174`)
A list that violates any rule is rejected **as a whole**.
```json
{
  "schema": 1,
  "updated": "2026-10-09",                       // YYYY-MM-DD, read as UTC day, year 2024–2100
  "sources": ["…"], "notes": ["…"],             // ignored by the reader
  "claude": {
    "webSearch": 0.01,                           // USD per web search request, 0–1
    "usOnlyMultiplier": 1.1,                     // inference_geo == "us", 1–3
    "models": [                                  // 1–500 entries, ids must start with "claude-"
      {"id":"claude-opus-5-5","input":4,"output":20,"cacheRead":0.2,"cacheWrite":5,"cacheWriteLong":8,"fastMultiplier":2},
      {"id":"claude-sonnet-4-5","input":3,"output":15,"cacheRead":0.3,"cacheWrite":3.75,"cacheWriteLong":6,
       "longContext":{"above":200000,"inputMultiplier":2,"outputMultiplier":1.5}}
    ],
    "plans": [ {"match":"max_20x","name":"Max 20×","monthly":200}, {"match":"max_5x","name":"Max 5×","monthly":100},
               {"match":"max","name":"Max"}, {"match":"enterprise","name":"Enterprise"},
               {"match":"team","name":"Team"}, {"match":"pro","name":"Pro","monthly":20} ]
  },
  "codex": {
    "models": [ {"id":"gpt-5.6","input":4,"output":20,"cacheRead":0.4,"cacheWrite":5,"fastMultiplier":2,
                 "longContext":{"above":272000,"inputMultiplier":2,"outputMultiplier":1.5}}, … ],   // ids must NOT start with claude-
    "plans": [ {"match":"free","name":"Free","monthly":0}, {"match":"go","name":"Go","monthly":8},
               {"match":"plus","name":"Plus","monthly":20}, {"match":"pro","name":"Pro"},
               {"match":"business","name":"Business"}, {"match":"enterprise","name":"Enterprise"},
               {"match":"edu","name":"Edu"} ]
  }
}
```
Field rules:
* Prices are USD per 1M tokens, in the range 0–1000. A missing `cacheWrite` falls back to `input`, and a missing `cacheWriteLong` to `cacheWrite`. `fastMultiplier` is 1–10 (default 1).
* `longContext.above` is 1000–10,000,000, and both multipliers are 1–10.
* Ids are lowercase `[a-z0-9._-]`, at most 64 characters. Plan `match` is at most 40 characters, `name` 1–24, `monthly` 0–100,000.

### 6.3 Model matching (`AU/AgentPricing.swift:197-239`)
* **Normalize**: lowercase and trim. Cut everything before `claude-`. For Claude ids, `.` becomes `-` (Copilot writes `claude-sonnet-4.5`). Drop everything up to the last `/` (provider prefixes). Cut at the first `@` or `[`.
* Table: the id starts with `claude-` → the Claude table; anything else → the codex/OpenAI table. This includes OpenCode and Copilot models, so non-Anthropic and non-OpenAI models (Gemini and the like) are **unpriced**.
* **Family match**: the longest table id that is a prefix, at a word boundary.
  * The next character must be `-`, `_`, `:` or the end.
  * If digits follow that separator, there must be ≥ 4 of them (a date snapshot). So `claude-opus-5` does not match `claude-opus-5-6`.
  * If the remainder contains a sibling word (`mini nano pro lite research search audio realtime transcribe tts cyber image`), the model is **unpriced**, which is better than a wrong price.

### 6.4 Cost formula (`AU/AgentPricing.swift:241-264`)
```
mult = (fast ? fastMultiplier : 1) × (domestic ? usOnlyMultiplier : 1)
inRate = outRate = mult
if !aggregate && longContext && (input+cacheWrite+cacheRead) > above: inRate×=inputMultiplier; outRate×=outputMultiplier
long = min(longCacheWrite, cacheWrite)
cost = [input·pIn·inRate + (cacheWrite−long)·pCW·inRate + long·pCWL·inRate + cacheRead·pCR·inRate + output·pOut·outRate]/1e6
       + webSearches·webSearch
savings = cacheRead·max(0,pIn−pCR)·inRate/1e6
```
An aggregate record with 0 tokens costs 0. An unknown model gets `cost=nil`, and the record counts as "unpriced". Totals with any unpriced record render as `≥ $X`, and breakdowns then sort by tokens instead of cost (`AU/AgentUsageSummary.swift:42-44,61-62`).

### 6.5 Display names (`AU/AgentPricing.swift:269-299`)
* Claude: `claude-opus-5-5` → "Opus 5.5". Date and `v`-suffix parts are dropped, and so is `latest`.
* GPT: `gpt-6-astra` → "GPT-6 Astra". Dated snapshots and `latest` are dropped.
* Anything else is shown raw (normalized).

---

## 7. Persistence (archive)

* Mac location: `~/Library/Caches/<bundle id>/AgentUsage/agent-usage.bin`, mode 0600 in a 0700 directory (`AU/AgentUsageArchive.swift:25-52`). Windows: `%LOCALAPPDATA%\<app id>\cache\AgentUsage\agent-usage.bin` (a cache, safe to delete).
* Contents: the provider set, every usage record (key, record, billable, source log paths), the newest limits per provider, the Codex plan and its observation date, open and waiting turns, and per-log cursors.
  * Each cursor holds path, provider, offset at a line boundary, inode, a "discarding oversized line" flag, mtime, parser state, and a fingerprint.
  * The fingerprint is FNV-1a 64 over the little-endian offset plus the first 4 KiB and the last 4 KiB before the offset (`AU/AgentUsageStore.swift:702-725`).
* **No text** (prompts, replies) is stored, and nothing from OpenCode.
* Format: magic `VAUA`, format 4, LEB128 varints (zig-zag for signed), a deduplicated string table, f64 dates (seconds since 2001-01-01), and an FNV-1a checksum trailer.
  * A build stamp (`CFBundleShortVersionString-CFBundleVersion [commit]`) must match, so **any app update invalidates the archive** and triggers a full re-read (`:31-39,180`).
  * **The Windows port does not need byte compatibility.** Any versioned format will do, e.g. bincode or SQLite in `%LOCALAPPDATA%`. Keep the semantics: resume only for the same provider set and the same build, and drop cursors whose inode or fingerprint changed.
* Resume rules (`AU/AgentUsageArchive.swift:74-90`):
  * Logs that are gone or rewritten give back their records.
  * Logs that shared a record with a gone log are re-read in full.
  * Limits keep the newest reading. Records are repriced and the horizon is trimmed.
* Windows identity: use the `nFileIndex` from `GetFileInformationByHandle` as the inode. **Open logs with `FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE`**, so Claude, Codex and Copilot can still rotate or delete their files.

---

## 8. Aggregation (snapshot)

`AgentUsageSnapshot` (`AU/AgentUsageSummary.swift:84-106`) contains:
* `periods` for `today` (1 day), `week` (7) and `month` (30). Each period holds totals, per-provider totals, model shares (id `provider:displayName`) and project shares, sorted by cost when the period is fully priced and by tokens otherwise.
* `days`: 91 local-calendar day buckets. `hours`: 24 buckets for today.
* `limits` and `plans` per provider.
* `live`: open turns, sorted by start.
* `claudeBlock`: the 5-hour block (§2.3).
* `burnRate`: the last 30 min ×2, i.e. per hour.
* `lastActivity` per provider, and `seen` (providers with any record, limit or turn).

Notes:
* Providers without `seen` get **no cards**, except Codex when accounts are added (`NAV:34-37`).
* The snapshot is republished on a tick only if inputs changed, or if the clock moves it: a non-empty burn rate, an active Claude block, or a day change (`AU/AgentUsageSummary.swift:208-211`).
* The incremental cache (`AgentUsageSummaryCache`, `:256-398`) is a performance detail. On Windows, rebuilding from SQLite or memory each publish is acceptable if fast.

**Live turn** (`AgentLiveSession`): id is the log file path, plus provider, `started`, `lastActivity`, model, project, tokens and cost.
* It accumulates usage dated ≥ start − 1 s.
* After 10 min of silence it moves to `waiting` and is no longer shown as working, but resumes if work continues.

---

## 9. Events and alerts (notices in the island)

These are emitted only once `reportsTransitions` is true and pass user filters (`AU/AgentUsageService.swift:638-695`). They are rendered by `NS:4140-4176`.

| Event | Condition | Notice title / detail | Extra |
|---|---|---|---|
| `finished` | A turn ended *completed*, ended ≤ 5 min ago, and duration ≥ `notchAgentsFinishMinimum` (default 60 s; choices 0/30/60/120/300; 0 = "Any length"). Alert on = `notchAgentsFinishAlert` (default on). | "<Provider> finished" / "4m 12s · $1.23" (cost omitted if 0); provider symbol, agent tint | mascot `.celebrate` |
| `limitWarning` | A window's `usedPercent` crossed ≥ threshold (default 80; choices 50/75/80/90/95) since the previous reading of the same window id, or the window renewed and is again above, or an account reading shows a window that began after the previous reading (`AU/AgentUsageSummary.swift:441-457`). Alert on = `notchAgentsLimitAlert` (default on). | "<Provider> · Session/Week[ · Opus]" / "20% left" or "80% used" (follows the display setting); `exclamationmark.triangle.fill` | — |
| `limitReset` | A warned window's `resetsAt` passed, or it was renewed early (banked reset): usage fell by > 1 point, or the reset moved > 60 s later after the old reset time (`:464-468`) | "<Provider> · <window>" / "Limit renewed"; `arrow.clockwise` | mascot `.celebrate` |
| `budgetReached` | Today's API value ≥ `notchAgentsDailyBudget` (choices 0=Off/5/10/25/50/100/250 USD); once per local day | "Daily budget" / "$27.40"; `dollarsign.circle.fill` | — |

Window label for `other` kinds: the duration (e.g. "1 d"), else the scope, else "Limit".

---

## 10. UI

All colors are SwiftUI `Color(red:green:blue:)` in the 0–1 range, sRGB. The island is always dark (white text at varying opacities on black).

### 10.1 Provider tints (`NAC:8-17`) and limit tint

| Provider | RGB | Hex |
|---|---|---|
| Claude | 0.85, 0.47, 0.34 | `#D97857` |
| Codex | 0.49, 0.60, 1.00 | `#7D99FF` |
| OpenCode | 0.06, 0.73, 0.51 | `#0FBA82` |
| Copilot | 0.30, 0.78, 0.68 | `#4DC7AD` |

* Limit tint (`NAC:21-25`): used ≥ 95 % → red, ≥ 80 % → orange, otherwise the provider tint.
* Fallback SF Symbols when the vendor app is not installed: Claude `sparkle`, Codex `chevron.left.forwardslash.chevron.right`, OpenCode `terminal`, Copilot `infinity` (`AU/AgentUsageModels.swift:22-29`).
* On Mac the provider **mark** is taken at runtime from the vendor app bundle, because no vendor artwork ships (`NAC:337-395`):
  * Claude `TrayIconTemplate` in `com.anthropic.claudefordesktop`.
  * Codex `chatgptTemplate` in `com.openai.codex` / `com.openai.chat`.
  * Otherwise the app icon. OpenCode bundles: `ai.opencode.desktop[.beta|.dev]`. Copilot bundle: `com.github.githubapp`.
  * **Windows:** extract the icon from the vendor exe with `SHGetFileInfo`/`ExtractIconEx`, or use neutral glyphs (lucide icons). Trademark caution: do not ship vendor logos.

### 10.2 Page layout (`NAS:253-331`, `NAV`)
* Cards are `limits, spend, live, trend, models, projects, activity, resets`, in a user-defined order stored in `notchAgentsCardOrder` (comma-separated raw values). Cards missing from the saved order are appended at the end. `notchAgentsHiddenCards` is a comma-separated set (`NAS:102-117`).
* Tiles:
  * `limits` expands to one tile per provider that reports limits and is visible: Claude, Codex, OpenCode. For Codex with accounts, there is one tile per account.
  * `resets` exists only while Codex is enabled (`NAS:266-281`).
* Rows:
  * Spacing is 10. `trend` and `activity` are full-width; the other cards pair in reading order when the page width is ≥ 390, otherwise one card per row.
  * Row height is 96, or 118 for chart rows, or 116 if the row holds an account card or the Resets card naming the default account.
  * The page scrolls when the content is taller than the page (`NAV:61-68`).
* Card chrome: padding 12 horizontal / 10 vertical. Corner radius 18 on the island's "control surface" material, a subtle translucent white fill (`NAC:28-38`).
* Header: height 17, gap 6. Icon 10 pt semibold in a 13-wide frame, or the provider mark 13 pt in a 17×17 frame. Title 10.5 pt semibold, white 90 %. Accessory on the trailing edge (`NAC:40-69`).
* Chip: 9 pt semibold, tint 90 %, padding 5 / 1.5, capsule filled with tint 14 % (`NAV:156-169`).
* Meter: capsule track white 13 %, fill tint 92 % (45 % when dimmed), height 4 (+5 for the pace tick).
  * Pace tick: 1.5 × (h+5), white 85 %, at the elapsed fraction of the window, drawn only between 2 % and 98 % (`NAC:79-106`).
  * In "Left" mode the pace tick is mirrored: 1 − elapsed.
* Period menu: "Today / 7 days / 30 days" in the island's top band, beside the camera. It shows while any period-following card is visible (spend, trend, models, projects) and is stored in `notchAgentsPeriod` (`NAV:69-95`).
* States:
  * Loading (not loaded and no Codex accounts): spinner + "Reading usage…" at 11 pt.
  * No providers seen: the empty view with `sparkles` and "No usage from Claude Code, Codex, OpenCode or GitHub Copilot yet. It appears here as soon as any of them works on this Mac."
  * All cards hidden: "Choose what this page shows in Dynamic Island settings."

### 10.3 Cards

**Limits card** (`NAV:186-450`):
* Header: provider mark, provider name (or "Codex · <account>"), plan chip, and a pulse dot when the provider is working.
* Up to 2 rows: the session window, plus whichever non-session window has the highest usage.
* Each row shows:
  * label 10 pt medium (white 85 %): "Session", "Week", "Week · Opus", or a duration for `other`;
  * a countdown `⟳ 2h 13m` at 9.5 pt secondary, prefixed `≈ ` when estimated. The countdown uses the 2 largest units, minimum 1 min, and never shows seconds;
  * the percent at 12 pt semibold rounded monospaced, white unless orange/red;
  * a 4 pt meter.
* Tooltip: "label · X% used · Y% left · <date time> [· Estimated from this Mac] [· Updated …]".
* A reading older than 10 min adds the caption "Updated 12 min ago" at 9.5 pt tertiary. A stale reading (Claude ≥ 30 min, account ≥ 10 min) dims the rows to 60 %.
* No windows:
  * Claude: the block estimate, or "No session running", plus the link "Set up plan limits… ↗", which opens Settings → Agents.
  * OpenCode: the today summary.
  * Codex: "Limits appear after the next reply" plus last used.

**Spending card** (`NAV:466-549`):
* Big value `[≥ ]$12.34`, 22 pt medium rounded, followed by "API value" at 9.5 pt secondary.
* Below it, a 4 pt stacked split bar of providers in their tints. Each provider gets a minimum 4 pt sliver, with 2 pt gaps.
* Footer: "1.2M tokens · 87% from cache", 10 pt secondary. Tooltip "$X saved by the cache".
* In the 30-day period only, the header shows plan-multiple chips per provider, e.g. `3.4×`. The multiple is API value ÷ plan monthly price, shown only when the price is > 0. Tooltip "3.4× the price of Claude Max 20×".

**Now card** (`NAV:553-627`):
* Up to 2 live turns, with a `+N` chip and a pulse.
* Each row: glyph 9 pt; project name (or provider) 11 pt semibold, middle-truncated; elapsed clock `12:05` 11 pt in the provider tint; subline "Opus 5.5 · 20K written · $4.56" 9.5 pt secondary.
* "Written" means **output tokens only**. The tooltip has the total tokens and the cache %.
* When idle: one row per provider showing the relative last activity, or "Idle".

**Trend card** (full width, `NAV:631-700`, `NAC:204-248`):
* Stacked bars per bucket: 24 hourly bars for Today, otherwise the last 7 or 30 days. The gap is 2 when there are more than 24 bars, else 4. Corner radius min(3, w/2). Hovering dims the other bars to 50 %.
* Header caption: "Today · $12.34"; on hover, "<date> · $value · N tokens".
* Axis at 8.5 pt tertiary: weekday letters for 7 days, otherwise the first, middle and last labels.

**Models / Projects cards** (`NAV:704-751`):
* Top 3 rows: name 10.5 pt medium; a bar up to 44 pt wide, minimum 3, 4 high, in the tint (white for projects); value 10 pt (cost or tokens), min width 40.
* Empty state: "Nothing in this period".

**Activity card** (full width, `NAV:755-835`, `NAC:250-331`):
* Heatmap of 91 days in week columns, starting on the locale's first weekday. Square cells with 2.5 spacing and corner radius min(2.5, cell/3).
* Cell opacity by quartile of active days: 0 → 7 %, then 28 / 45 / 65 / 90 %. Today has a 45 % white border; a hovered day has 90 %.
* Right side: the 13-week total at 17 pt, "13 weeks", "Active days N", and "Busiest day <date · value>" (replaced by the hovered day).
* A streak chip `🔥N` (SF `flame.fill`, orange) appears when the streak is > 1.

**Resets card**: see §3.4.
* Count chip, or a spinner while checking. Content texts 10.5 pt.
* Pill buttons: 20 high, padding 9, Codex tint at 20 % for prominent buttons, white 10 % otherwise (`NAV:842-1003`).
* Tooltip: "A reset renews the Codex session and weekly limits at once…".

### 10.4 Closed-island strip while an agent works (`NASt`, `NAS:212-249`)
* Shown when `notchAgentsLiveActivity` is on (default) and `snapshot.live` is non-empty.
* Left wing: the marks of the working providers, in provider order. A mark is 14 pt with one provider, 11 pt with two or more, and each sits in a frame of size×1.45+1 wide.
* Right wing: the reading, text size min(15, contentHeight − 7), medium weight, monospaced digits, in the first working provider's tint, shrinking to 0.6 if needed.
* Wing width: `max(readingWidth, marksWidth) + 6`, measured on the "shape" of the reading (every digit → `0`) so the width does not jitter. Clamped to 44…80 (`NAS:216-218`, `NS:771-789`).
* Readouts (`notchAgentsReadout`, default `elapsed`):

  | Readout | Shows |
  |---|---|
  | `elapsed` | stopwatch `m:ss` / `h:mm:ss` since the earliest live start |
  | `tokens` | Σ output tokens of the live turns, formatted `4.2K` / `48K` / `1.2M` |
  | `cost` | Σ live cost `$1.23` |
  | `limit` | the focused window of the first working provider, as % left or % used; falls back to elapsed |

* Click opens the AI page.
* Glyph animation (`NAAV:151-168`):
  * The working glyph "breathes": scale 0.84→1.0 and opacity 0.7→1.0, eased by a cosine swing with a half-period of 1.2 s.
  * The pulse dot's ring spreads to scale 1.9 while fading from 0.6 to 0, with a 1.6 s period and sin ease-out.
  * Animations step at most 30 fps (`NotchDecorativeClock.swift:16`) and stop when the view is hidden, occluded or Reduce Motion is on.
  * **Windows:** CSS keyframes, honouring `prefers-reduced-motion`.

### 10.5 Resting island wings (`NASt:120-163`)
* Used when the island's idle content is set to Agents.
* Left: an 11×11 ring (line width 2), filled to % left or % used. It shows the most-spent *focused* window across all providers (`NAS:158-168`).
* Right: the percent at 9 pt.
* With no limits known: the first seen provider's mark, and today's API value.
* Ring track: white 18 %.
* `notchAgentsLimitFocus` is one of `mostUsed` (default), `session` or `weekly`. A model-scoped window is never "chosen" (`NAS:139-153`).

### 10.6 Number formats (`NAS:336-409`)
* Cost:
  * `$` + 2 decimals below 100, 0 decimals from 100.
  * From 10,000: `$12K`.
  * Locale grouping. The currency is always USD.
* Tokens: below 1000 the plain number; then K/M/B with 1 decimal below 10 and none above, **truncated (rounded down)**.
* Percent: 0 decimals.
* Durations: abbreviated, 2 units: "4m 12s", "1h 05m", "2d 3h".

---

## 11. Settings (Mac `UserDefaults` → Windows app settings store)

Registered defaults: `DEF:1479-1497`. Keys: `DEF:870-889`.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `notchAgentsEnabled` | Bool | `false` (migrated to `true` for existing island profiles, `DEF:2044-2054`) | section on |
| `notchAgentsClaude` / `…Codex` / `…OpenCode` / `…Copilot` | Bool | `true` | read this provider (at least one must stay on) |
| `notchAgentsCodexProfiles` | String (JSON array) | `""` | CODEX_HOME accounts (§3.5) |
| `notchAgentsCardOrder` | String CSV | `""` | card order |
| `notchAgentsHiddenCards` | String CSV (sorted) | `""` | hidden cards |
| `notchAgentsPeriod` | `today`/`week`/`month` | `today` | period for spend/trend/models/projects |
| `notchAgentsLimitDisplay` | `remaining`/`used` | `remaining` | "Show limits as Left/Used" |
| `notchAgentsLimitFocus` | `mostUsed`/`session`/`weekly` | `mostUsed` | "Limit to show" (strip limit readout + resting wing) |
| `notchAgentsLiveActivity` | Bool | `true` | "Show it in the closed Dynamic Island" |
| `notchAgentsReadout` | `elapsed`/`tokens`/`cost`/`limit` | `elapsed` | "Beside the camera" |
| `notchAgentsFinishAlert` | Bool | `true` | "When a task finishes" |
| `notchAgentsFinishMinimum` | Double s | `60` | 0/30/60/120/300, clamped 0–3600 |
| `notchAgentsLimitAlert` | Bool | `true` | "Near a plan limit" |
| `notchAgentsLimitThreshold` | Double % | `80` | 50/75/80/90/95, clamped 1–100 |
| `notchAgentsDailyBudget` | Double USD | `0` (off) | 0/5/10/25/50/100/250 |
| `notchAgentsPriceUpdates` | Bool | `true` | daily price download |

Settings panel layout (`NASet:42-147`):
1. Description: "Reads the usage Claude Code, Codex, OpenCode and GitHub Copilot record on this Mac and the plan limits the Claude app saves. Prompts, replies and files are never kept, and your usage never leaves your Mac."
2. Four provider rows. Each has a 26×26 mark tile (tint 14 %, radius 7), the name, a caption "Found on this Mac" / "Not found on this Mac" (the log root exists), and a switch.
3. Codex accounts editor (when Codex is on, `CAS`):
   * Per account: a name field and the path, with a remove button.
   * Buttons "Find local accounts", "Add folder…" (folder picker showing hidden files) and "Check limits".
   * Notes: "With no accounts added, Codex limits come from its local sessions." / "The Resets card still belongs to the default Codex account."
4. Cards grid: an adaptive grid with a 104 minimum, draggable to reorder, toggle to include. VoiceOver move up/down actions are available.
5. "Show limits as" (Left/Used) and "Limit to show" (Most used/Session/Week).
6. "While an agent works": live strip switch → "Beside the camera" (Time / Tokens written / API value / Limit), plus a **live sample strip**.
   * The sample strip is 180×24, a black notch shape with corner radius 9 and a 64 pt camera gap (`NASet:288-328`).
   * Without a live turn it shows example data: 754 s elapsed, 1.18M input, 20K output, $4.56.
7. Alerts: finish switch → "For tasks longer than"; limit switch → "Warn at" (X% used); "Daily API value budget"; the value note; "Keep prices up to date" with the caption "Downloads the public price list from GitHub once a day… Price list of <day>".
8. "Claude plan limits" status, re-evaluated every 30 s (`NASet:171-208`):
   * Account reading < 30 min → green "Claude Code · Updated …".
   * App reading < 30 min → green "Read from the Claude app, updated …".
   * No app and nothing ever checked → info "Plan limits come from Claude Code or from the Claude app, which is not on this Mac…" + "Get the Claude app" (`https://claude.ai/download`).
   * Otherwise → orange "The Claude app last checked them …" + "Claude checks your limits only while its icon is in the menu bar…" + "Open Claude".
   * When the account reading is not fresh, the last Claude Code failure reason is listed below: "Claude Code was not found on this Mac." / "Sign in to Claude Code so it can check your account's limits." / "Update Claude Code…" / "Claude Code could not check… It tries again later."
   * Plus the privacy note `claudeLimitsPrivacy`.

All English strings: `STR:191-288`, `CAStr:33-51`. They exist in 15 locales.

---

## 12. Windows path cheat-sheet

| Data | macOS | Windows (proposed) | Confidence |
|---|---|---|---|
| Claude transcripts | `~/.claude/projects` | `%USERPROFILE%\.claude\projects` | high |
| Claude session registry | `~/.claude/sessions/<pid>.json` | `%USERPROFILE%\.claude\sessions\<pid>.json` | medium (field `pidDomain` value unknown) |
| Claude profile | `~/.claude.json` | `%USERPROFILE%\.claude.json` | high |
| Claude user settings | `~/.claude/settings.json` | `%USERPROFILE%\.claude\settings.json` | high |
| Claude OAuth creds (never read) | Keychain (item name is Claude Code's own, unverified) | `%USERPROFILE%\.claude\.credentials.json` | medium — verify; app must not read it |
| Claude CLI binary | `~/.local/bin/claude` … | `%USERPROFILE%\.local\bin\claude.exe`, `%APPDATA%\npm\claude.cmd` | medium |
| Claude app limits history | `~/Library/Application Support/Claude/plan-usage-history.json` | `%APPDATA%\Claude\plan-usage-history.json` | **low** |
| Codex logs | `~/.codex/sessions`, `~/.codex/archived_sessions` | `%USERPROFILE%\.codex\…` or `%CODEX_HOME%\…` | high |
| Codex auth (never read; only validated for profiles) | `~/.codex/auth.json` or Keychain | `%USERPROFILE%\.codex\auth.json` or Windows Credential Manager via keyring | medium |
| Codex CLI | app bundle / `~/.local/bin/codex` | `%APPDATA%\npm\codex.cmd`, `codex.exe` on PATH | medium |
| OpenCode DB | `~/.local/share/opencode/opencode.db` | `%USERPROFILE%\.local\share\opencode\opencode.db` | medium — verify |
| Copilot events | `~/.copilot/session-state/<id>/events.jsonl` | `%USERPROFILE%\.copilot\session-state\<id>\events.jsonl` | medium — verify |
| Price cache | `~/Library/Application Support/<id>/agent-prices.json` | `%APPDATA%\<app>\agent-prices.json` | n/a (ours) |
| Usage archive | `~/Library/Caches/<id>/AgentUsage/agent-usage.bin` | `%LOCALAPPDATA%\<app>\cache\AgentUsage\…` | n/a (ours) |

Path handling: project names are derived from `cwd`. On Windows `cwd` will look like `C:\Users\me\repo`, so take the last component after normalizing `\` to `/`. Strip `\.claude\worktrees\…`. Case-insensitive comparisons are needed for dedup.

---

## 13. Open questions / risks

1. **No credential handling by design.** The Mac reads neither Keychain nor auth files; the CLIs authenticate themselves. Keep this on Windows. Do **not** read `.credentials.json` or Credential Manager to call Anthropic or OpenAI APIs directly; it would also break with token refresh. The only auth-adjacent thing done is the metadata/ACL validation of a CODEX_HOME's `auth.json`.
2. **`claude` headless control protocol on Windows** (`--input-format stream-json`, `control_request` `get_usage`, `skip_behaviors`). This is undocumented and verified only against Claude Code 2.1.30x on Mac. Verify that `claude.exe` answers it on Windows, that `.cmd` shims pass stdin/stdout cleanly, and that `cachedUsageUtilization.fetchedAtMs` is still written to `%USERPROFILE%\.claude.json`.
3. **`codex app-server` methods** (`account/read`, `account/rateLimits/read`, `account/rateLimitResetCredit/consume`) are an internal protocol and may change. Verify on the Windows Codex CLI. The redeem action **spends a real resource**, so test it carefully. It is idempotency-key protected.
4. **`plan-usage-history.json` on Windows is unverified.** The Windows Claude desktop app may not write it, or may write it elsewhere (`%APPDATA%\Claude`, `%LOCALAPPDATA%\AnthropicClaude`). If it is absent, Claude limits depend entirely on the CLI check.
5. **Session registry `pidDomain`.** The Mac skips anything that is not `"darwin"`. Learn the Windows value before filtering, or the registry-based turn closing silently does nothing. PID reuse on Windows is frequent, so compare process start times.
6. **File-change detection on Windows.** Appends by a process holding the file open may not raise `ReadDirectoryChangesW` events promptly, so keep the 2 s size/index poll. Opening logs without `FILE_SHARE_DELETE` can block Claude/Codex rotation or deletion.
7. **OpenCode and Copilot data dirs on Windows** (`~/.local/share/opencode`, `~/.copilot`) are inferred from their XDG-style code paths and need checking on a real install. Both may also honour `XDG_*` / `COPILOT_HOME` overrides that the Mac build ignores.
8. **`CLAUDE_CONFIG_DIR` / `CODEX_HOME` overrides** are not honoured for log roots on Mac. Only the fixed home paths are read (`AU/AgentUsageStore.swift:475-481`). A Windows user with relocated dirs would see nothing; consider adding them.
9. **Executable discovery on Windows** (npm `.cmd` shims, Volta, nvm-windows, Scoop `%USERPROFILE%\scoop\shims`, winget). This differs completely from the Mac list. The child env must keep `SystemRoot`, `PATHEXT` and `APPDATA`, or Node/Rust CLIs fail.
10. **Price list URL** points at the upstream `vorssaint/vorssaint-utils` repo. Decide whether the Windows port fetches from there (licence and trust) or hosts its own copy. The bundled list will go stale: `updated` is 2026-10-09, and the notes flag promotional prices ending 2026-11-21.
11. **Model naming from Copilot and OpenCode** (provider prefixes such as `anthropic/claude-…`, dots in versions) relies on the normalizer. Gemini, Grok and other non-Anthropic/OpenAI models are always unpriced, so totals show `≥`.
12. **Archive invalidation on every app update** causes a full re-read of 91 days of logs, which can be gigabytes. The Windows port should version its cache by parser version, not build number, to avoid long first loads after updates.
13. **Vendor marks.** The Mac borrows icons from installed vendor apps at runtime. On Windows, extracting exe icons is possible but has trademark and UX implications. Consider neutral glyphs.
14. **Time zones and day buckets** use the local calendar and the locale's first weekday. Claude renewals are snapped to UTC hours. Keep that distinction.
15. **Not verified here:** the exact `NotchControlSurface` material color and opacity, which are in shared island styling outside this section's files, and the strip's precise geometry inputs (`compactActivityContentHeight` etc.), which belong to the island geometry spec.
