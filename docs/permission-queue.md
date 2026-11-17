# Permission queue

Issue #269. When more than one permission ask waits, the Sessions list opens
with all of them in one queue instead of one card at a time. On Mac, Windows
and Linux.

## What you see

- "3 waiting" on top, then one row per ask: the session, the tool, and the
  command or file on one line. A session that asks several things at once
  (Claude Code's subagents, a Codex thread and its subagents) gets a row for
  each.
- Each row has its own Deny, Always and Allow. Always does what it does on the
  card: Claude Code's "don't ask again" rules, OpenCode's and Vibe's
  session-wide allow, and Codex's Always for the rest of the turn.
- A question, a plan, or a command that has to be read in full first (#224)
  shows Review instead. Review, or a click on the row, opens the session's
  page on that ask, even when it waits behind another one.
- **Allow all** allows only the asks that look safe. The rest stay in the
  queue, each with a short reason.
- A new ask joins at the bottom. It never moves the row or the page you are
  reading. When the island opens because of a new ask and more than one is
  waiting, it opens on the queue.

Rows only list asks the island can answer: Claude Code, Codex, OpenCode, and
Mistral Vibe through its app server. Agents that are answered in their own
terminal keep doing that.

## What Allow all leaves out

The check lives in `PermissionRisk.swift` (Mac) and `sessions/risk.rs`
(Windows and Linux). Both run the same cases from
`windows/src-tauri/src/sessions/risk_cases.json`. It is a list of what looks
dangerous, not a sandbox, and anything it can't place counts as risky.

| Reason on the row | What triggers it |
| --- | --- |
| Your settings ask first | A `permissions.deny` or `permissions.ask` rule in Claude Code's managed, account or project settings matches (`Bash(git push:*)`, `Read(./.env)`, `WebFetch(domain:…)`, `mcp__server`). |
| Deletes files | `rm`, `rmdir`, `del`, `Remove-Item`, `find -delete`, `git clean`, `git reset --hard`, `git restore`, a file change that deletes. |
| Outside the project | A read, write, redirect (`>`, `tee`), copy target, `cd` or `git -C` outside the session's folder, after `~`, `..` and symlinks. A session in the home folder or `/` has no project to stay in. |
| Uses the network | `curl`, `wget`, `ssh`, `git push/pull/fetch/clone`, `gh`, `npx`, package installs, `Invoke-WebRequest`, WebFetch, WebSearch. |
| Changes the system | `sudo`, `chmod`, `chown`, `kill`, `launchctl`, `systemctl`, `reg`, global npm installs, Codex asking for more sandbox access. |
| Runs code it builds | `eval`, `source`, a pipe into a shell or interpreter, `python -c`, `node -e`. A shell's own `-c` script is read like any other command. |
| Unfamiliar tool | Any tool the island doesn't know, MCP tools included. |
| Not checked yet | An ask published before it was looked at. |

Commands are split at `;`, `&&`, `||`, `|`, newlines, `$(…)` and backticks,
and each part is judged past wrappers like `env`, `nohup`, `xargs` and
`timeout`. Rules are read again at most every 5 seconds. A remote host's
sessions are judged without its settings or symlinks, since those live on the
host.

## How it fits together

- The session snapshot keeps `permission` (the first ask) and adds
  `laterAsks`. Every ask has a `risk`.
- `decide`, `inspect` and Codex's `answer` take any ask the session holds,
  not only the first, so rows can be answered in any order. Each ask waits the
  usual 0.8 s settle delay on its own, and inspection grants are kept for every
  held ask.
- Allow all sends one ordinary `decide(…, allow)` per safe, settled row, so
  each one passes the same checks as a click.

## Tests

- Mac: `./build.sh --test-suite=permission-security` (risk cases, rules,
  links, the queue model), `--test-suite=sessions`, `--test-suite=codex-sessions`.
- Windows/Linux: `cargo test --lib sessions::`, `bun run test`, and
  `npm run test:permission-queue` (the island page in headless Chromium:
  five asks, Allow all, an ask behind another, a new ask while a card is open).
