# Permission command inspection — issue #224

## Vulnerability and invariant

Permission previews previously discarded command suffixes while still offering Allow and Always. Claude and Codex clipped at 8,000 characters; OpenCode's plugin clipped at 4,000 before either platform received the command. Approving the visible prefix also approved the hidden suffix.

Approval now requires authoritative, complete content. A shortened command (or one containing invisible controls) also requires a backend-issued inspection capability for that exact pending request. Deny does not require inspection. The existing approval transports and agent responses remain in use.

## Content and integrations

- **Claude Code:** `permissionDetail` / `permission_detail` return the original text without clipping or trimming. Permission records retain it separately from the preview. Missing, empty or non-string Bash commands are incomplete. Tool input matching hashes the full input rather than an abbreviated detail.
- **Codex:** permissions retain the original command, including shell wrappers, quoting and whitespace. Wrapper peeling remains a convenience for activity summaries only. Context such as working directory and reason stays in the preview; it is not appended to the authoritative command. The last dispatch check runs under the client's state lock and accepts only the current request on the session's row, including subagent requests.
- **OpenCode:** both copies of the plugin send the whole command, explicit completeness, and its original Unicode scalar count. The readers validate the completeness marker and length. The plugin keeps the pipe writable until its reply arrives; ending a Windows pipe before the reply can lose the approval response. Old plugins, missing or malformed commands, and inconsistent metadata fail closed. Commands exceeding the 1 MiB budget are explicitly incomplete, with a bounded preview and original length. The plugin copies are tested for parity; no command is executed by these tests.

The preview limit is 8,000 Unicode scalars. It is not a completeness heuristic: completeness comes from the source contract and explicit integration metadata. The full original string is retained separately. Rust uses shared string storage when permission records are cloned; Swift strings share storage through copy-on-write. Recurring Windows snapshots contain only the preview and inspection metadata. Full text is fetched on demand.

Swift content equality compares the original UTF-8 bytes, so even canonically equivalent Unicode changes refresh the permission snapshot and invalidate the previous inspection. Existing OpenCode processes using an older plugin remain blocked until that owned plugin is updated through the existing installer and OpenCode reloads it; a legacy payload never gets grandfathered into approval.

The renderer has an explicit 1 MiB raw/annotated-content budget. Exceeding it blocks approval and explains the limitation; the expanded viewer never quietly truncates content to fit. The existing hook transport also rejects oversized/malformed messages rather than treating a partial message as an approvable request.

## Viewer and approval boundary

A command supplied alongside file-diff metadata remains the card's primary content; a diff cannot hide even a short command.

Both permission cards offer an explicit **Inspect full command** action. Until the complete content has loaded, Allow and Always remain disabled. Expansion never sends an approval. Windows keeps the restricted card inside the session viewport with persistent approval controls; the content alone scrolls, and rendering/viewport checks must succeed before approval is enabled. macOS presents a 600 by 420 point command viewer using the existing above-island modal dialog helper, with a separate hosting root for focus and readiness. It does not attach a sheet to the borderless island. The expanded view is scrollable, monospaced and selectable; React renders text children and SwiftUI uses `Text(verbatim:)`. There is no HTML injection, terminal emulation, shell evaluation or execution.

Newlines and tabs remain in place. ANSI controls, carriage returns, bidi controls and invisible direction/formatting characters are shown as `⟦U+XXXX⟧`. Literal annotation delimiters are themselves annotated, so literal text cannot impersonate an encoded control. The stored original is unchanged. The full viewer takes keyboard focus; Escape closes it, and approval buttons remain keyboard-accessible. Existing approval shortcuts do not carry inspection capabilities and cannot bypass the restriction.

The backend binds each capability to a session and SHA-256 revision of the request ID, tool, request lifetime, complete content and completeness metadata. A frontend boolean or fabricated token does not authorize anything. Allow and Always validate that binding immediately before dispatch. Codex performs this check independently inside its own client lock, covering programmatic and automatic approvals as well as UI actions. Changed/replaced/completed requests reject old capabilities. Closing the viewer revokes its capability; snapshot pruning and disconnect handling remove obsolete state. Simultaneous sessions have independent capabilities. A new queued Codex request does not revoke an unrelated request's inspection.

Codex turn auto-allow retains its existing behavior for ordinary requests. Restricted requests remain visible and are excluded from automatic approval. Its trusted background path cannot manufacture an inspection capability. The Always button for an inspected Codex request explicitly answers that request before enabling turn auto-allow. Question forms retain their existing answer path and are not mistaken for shell approvals.

Opening an agent's terminal does not grant or unlock permission. When full content is unavailable, the person can deny or use the agent's own interface; the island continues to refuse approval.

The resumed implementation is based on freshly fetched main `181b6ed5` (including #225). Work is isolated in branch `fix/224-secure-permissions-current`, worktree `build/issue-224-current`. The earlier worktree and the original user changes are preserved; no commits were created to synchronize them.

OpenCode custom permissions also carry an explicit source-completeness flag. Full `patterns` (including empty lines) and fallback metadata with unknown fields are preserved within the transport budget; legacy or malformed custom payloads are blocked. Length checks validate provenance rather than inferring completeness from a short preview. Both platforms embed the same tested plugin. The plugin hashes the complete request before deduplication: changed requests with the same ID replace the waiting connection and invalidate queued replies. Cancellation and session deletion invalidate queued approvals too; late transport callbacks cannot remove a newer request.

## Automated verification

Commands run on Windows from `windows/`, with the existing shared Cargo target/cache:

- `npm run typecheck`: passed.
- Changed TypeScript, React, CSS, plugin and verification scripts: Prettier check passed.
- Full frontend suite on the resumed, updated checkout: 926 tests passed in 102 files. The previously observed timer failure did not recur in this run.
- Rust Sessions suite: 167 passed, with two unrelated Windows-incompatible tests explicitly skipped. `accounts::prefs_are_checked` expects Unix absolute paths and also fails on the unchanged main checkout. `remote::the_script_runs_under_sh` requires `/bin/sh`, which is absent here. The initial unfiltered run exposed those failures.
- Security regressions cover limits, malicious-looking but harmless suffix fixtures, raw whitespace/Unicode/quoting, controls, malformed/legacy OpenCode data, resource failures, forged capabilities, stale/replaced requests, closing/reopening inspection, independent sessions, keyboard and auto-allow restrictions, and Deny. The actual OpenCode plugin has 28 execution tests with a mocked transport and no command execution.
- `cargo clippy --tests --locked -- -D warnings` also exposed existing warnings in unmodified hooks, process, locale and audio code. One warning introduced in the modified detail helper was corrected. Strict repository-wide clippy is not reported as passing. The final ordinary clippy run completed successfully with five warnings in those unchanged files and no warnings in the changed security code.

## Platform status and evidence

Windows full Tauri release build with embedded frontend passed against main `181b6ed5` and was installed locally with backups of the previous executable and settings. The new process path and binary hash were verified. The 15 actual-application checks passed on that build; `build/permission-security/results.json` records SHA-256 `71834cc2a505e86baf10f95668b8d168bbc17eb56c06b8f7e4692e73b36a9885`. The app was then restarted in normal mode; the verification debugging port was confirmed closed.

`windows/scripts/verify-command-inspection-native.mjs` uses the real installed Tauri application and backend with harmless permission requests whose replies terminate in the test process. It does not inject mock snapshots or execute commands. Checks cover short Allow; backend rejection of collapsed Allow/Always and forged tokens; keyboard rejection; full text and focus; visible approval controls and reachable horizontal/vertical suffixes; explicit expanded Allow/Always; close/reopen and incomplete Deny; complete custom patterns; mixed command/diff payloads; same-ID replacement; and expiry. Seven screenshots were captured from the actual application and visually reviewed.

**macOS remains unverified in this Windows environment.** `swift build` could not run because `swift` is not installed; AppKit/SwiftUI and the macOS SDK are unavailable. Native macOS build, tests and screenshots/recording are still required before claiming cross-platform verification complete. Source changes and regression tests cover macOS, including the real Codex client boundary against the existing fake daemon. Run `./build.sh --test-suite=permission-security`, `./build.sh --test-suite=codex-sessions`, and the Sessions suite on a Mac, then verify the native card visually.

Reviewer evidence (all from the installed Tauri build; copied unchanged from the local capture artifacts):

- [01-short-command.png](evidence/issue-224/01-short-command.png)
- [02-shortened-blocked.png](evidence/issue-224/02-shortened-blocked.png)
- [03-full-command-top.png](evidence/issue-224/03-full-command-top.png)
- [04-full-command-suffix-allow-enabled.png](evidence/issue-224/04-full-command-suffix-allow-enabled.png)
- [05-incomplete-deny-enabled.png](evidence/issue-224/05-incomplete-deny-enabled.png)
- [06-horizontal-suffix-controls-visible.png](evidence/issue-224/06-horizontal-suffix-controls-visible.png)
- [07-controls-and-inert-text.png](evidence/issue-224/07-controls-and-inert-text.png)

Screenshots are actual application captures. The user subsequently authorized a pull request; the seven captures and [native verification results](evidence/issue-224/results.json) are included for reviewers. macOS evidence remains outstanding. The PR is a draft pending that platform verification; merging and publishing a release are not authorized.

## Changed files

New security implementation and tests:

- `Sources/Openbar/Services/ClaudeSessions/PermissionInspection.swift`
- `Tests/PermissionInspectionTests.swift`
- `windows/src-tauri/src/sessions/inspection.rs`
- `windows/src/sessions/commandInspection.test.tsx`
- `windows/scripts/opencode-permission.test.mjs`
- `windows/scripts/verify-command-inspection-native.mjs`
- `docs/permission-command-security.md` and `docs/evidence/issue-224/` (report and actual Windows evidence)

Updated macOS integrations and UI:

- `Sources/Openbar/Services/ClaudeSessions/{ClaudeSessionModels,ClaudeHookMessage,ClaudeSessionService,CodexSessionState,CodexSessionClient,AgentHookMessage,AgentOpenCodePlugin}.swift`
- `Sources/Openbar/Services/Notch/NotchClaudeSessionsSupport.swift`
- `Sources/Openbar/UI/Notch/NotchClaudeSessionDetailView.swift`
- `Sources/Openbar/UI/Notch/NotchComponents.swift` (reuses the existing above-island modal helper)
- `Tests/{AgentHookSessionTests,ClaudeSessionEngineTests,CodexSessionTests,MetricsTests}.swift`
- `build.sh` (includes the new production security source in the standalone test build)

Updated Windows integrations and UI:

- `windows/src-tauri/src/sessions/{models,hook_message,hook_agents,service,mod}.rs`
- `windows/src-tauri/src/sessions/codex/{state,client}.rs`
- `windows/src-tauri/src/sessions/opencode-plugin.js`
- `windows/src-tauri/src/lib.rs` (registers the inspection commands)
- `windows/src/sessions/{PermissionCard.tsx,store.ts,types.ts,permissionKeys.ts,presentation.ts,sessions.css}`
- `windows/src/shared/bridge.ts`
