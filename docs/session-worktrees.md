# Session worktrees

A session started from **New session** runs in a git worktree of its own, so several agents on one repository never edit the same checkout. The **Own worktree** switch sits under the first prompt. It is on by default, and the island remembers the choice for each agent.

## How each agent gets one

| Agent | Worktree | Where | Branch |
|---|---|---|---|
| Claude Code (Background and Terminal) | `claude --worktree <name>` | `<repo>/.claude/worktrees/<name>` | `worktree-<name>` |
| Codex (Background and Terminal) | made by Openbar | `<repo>/.openbar/worktrees/<name>` | `openbar/<name>` |
| Gemini CLI, Copilot CLI, OpenCode, Cursor | made by Openbar | `<repo>/.openbar/worktrees/<name>` | `openbar/<name>` |

Openbar makes worktrees itself where the agent has no reliable flag of its own:

- Codex's app-server (`thread/start`, which Background uses) takes no worktree option. `codex --worktree` puts a detached `HEAD` in `~/.codex/worktrees/<id>/<repo>`, which loses commits easily and has no name you chose, so Codex uses Openbar's in both modes.
- Gemini CLI's `--worktree` only works with `experimental.worktrees` turned on. Copilot CLI, OpenCode and `cursor-agent` have none.

Worktrees start from the last commit (`HEAD`). Uncommitted changes in your checkout stay there, and the New session page says so. Ignored files such as `.env` aren't copied.

Openbar adds `/.openbar/` to the repository's own `.git/info/exclude`, never to a shared `.gitignore`, so `git status` stays clean.

## Names

The name comes from the field next to the switch, or from the first prompt (`Fix the login bug` becomes `fix-the-login-bug`, accents folded to ASCII, at most 40 characters), or `session`. A name already used by a worktree folder, a `worktree-*`/`openbar/*` branch, or a session started in the last two minutes gets `-2`, `-3` and so on, so two sessions started at once never collide.

## In Sessions

- The row shows the worktree name where it used to show the branch. The project still groups under the repository: every `/.claude/worktrees/`, `/.openbar/worktrees/` and `/.gemini/worktrees/` path resolves to the repository it sits in.
- The session page has **Open folder**, **Copy branch**, and for Openbar's own worktrees **Remove worktree** once the session isn't working.
- Diffs, ⌘Y and Open/Reply use the session's own folder, which is the worktree.

## Removing

- **Remove** on a Claude Code background session runs `claude rm`, which deletes its worktree when that's safe. When the worktree has uncommitted changes or commits found on no remote, `claude rm` keeps it and Openbar asks: *This worktree has unsaved changes. Remove anyway?* Saying yes drops the uncommitted changes (`claude rm` has no flag for them), then passes the `--discard-unpushed` value `claude rm` printed.
- **Remove worktree** on Openbar's own worktrees runs `git worktree remove` and `git worktree prune`, asking the same question first when there are changes. The `openbar/<name>` branch is kept, so commits made in the worktree survive.
- Openbar never removes a worktree it didn't make.

## Folders that aren't repositories

The switch is greyed out with *Not a git repo, so the session runs in the folder.*, and the session starts in the folder as before. A bare repository counts as no checkout.

## Checked (2026-11-06, macOS, Claude Code 2.1.318, Codex 0.164.0)

- `claude --bg --worktree <name>` starts in `<repo>/.claude/worktrees/<name>` and `claude agents` lists it with that `cwd`. A trusted repository's worktree is trusted too.
- `claude rm` on a worktree with changes exits 1 and prints, on standard output, `kept <id> — its worktree is still at …` plus either *uncommitted changes* (no flag) or `--discard-unpushed <commit>@<id>`.
- Two Claude Code sessions and one Codex thread started on one repository each changed the same file in their own worktree; the main checkout stayed unchanged.

Windows runs the same rules (`windows/src-tauri/src/sessions/worktree.rs`) with `git.exe` from `PATH` or Git for Windows' usual folders.
