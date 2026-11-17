# New session in steps and safe commands (#270)

**+ New session** starts a session in four steps. The steps sit across the top of the page, and a click on any of them goes there.

1. **Project**: where the agent works.
2. **Agent**: Claude Code, Codex or another agent, with its account and model.
3. **Options**: its own worktree, safe commands, and Background or Terminal.
4. **Prompt**: the first thing to say.

Start works from any step. A background session needs a first prompt, so Start goes to step 4 when the prompt is still empty.

## What each project remembers

Each project keeps what it last started with: agent, account, model per agent, worktree on or off, Background or Terminal, and its safe commands. A project started before shows a small arrow in the list, and picking it goes straight to the prompt. For a repeat session you pick the project, type the prompt and press Start.

A project with nothing saved starts from the last choices of any project, as New session did before.

- Mac: `notchNewSessionByProject` in the app's defaults, as JSON by folder path (`NewSessionChoices.swift`). Paths belong to this Mac, so the key stays out of settings backups.
- Windows and Linux: `openbar.newSession.byProject` in the island's local storage (`newSessionChoices.ts`). Windows paths are compared without case.
- The 50 most recently used projects are kept.

## Safe commands

Most permission prompts ask the same things again and again: may I run the tests, the build, `git diff`. Safe commands are allowed for the whole session from its first turn.

The list starts from what the project has:

| Found | Suggested |
| --- | --- |
| `package.json` scripts `build`, `test`, `lint`, `typecheck` | `npm run …`, or `bun`, `pnpm`, `yarn` by lockfile |
| `Cargo.toml` | `cargo build`, `cargo test`, `cargo clippy` |
| `Package.swift` | `swift build`, `swift test` |
| `go.mod` | `go build`, `go test`, `go vet` |
| `Makefile` | `make`, and `make test` / `make lint` when those targets exist |
| a git repository | `git status`, `git diff`, `git log` |

Each entry is a chip you can turn off. **+** adds your own command. A command you add has to be plain words: quotes, `;`, `|`, `&`, `>`, `<`, `$`, backticks, brackets and `*` are refused, so a chained or redirected command can never get onto the list.

Anything that isn't on the list still asks on the island as before.

### How each agent gets the list

- **Claude Code**: `--allowedTools "Bash(<command>:*)" …` at start, for `claude --bg` and for a terminal session. These rules hold for that session only and change no settings file.
- **Codex**: Codex has no per-thread allow list at start, and its execpolicy rules (`~/.codex/rules`) are global. The island answers a matching `commandExecution` approval with `acceptForSession`, Codex's own session cache, so the card never shows. The command is unwrapped from the shell Codex runs it in (`/bin/zsh -lc '…'`, `bash -c`, `powershell -Command '…'`, `cmd /c`) and must start with every word of an entry. A thread started in the background gets the list by its id. A Codex in a terminal gets it when it is the first new thread in that folder within two minutes.
- **Other agents**: the option is hidden. Gemini CLI and Copilot CLI have allow flags, but they couldn't be checked on a real install yet.

On Windows, Codex runs commands in PowerShell, so an entry like `mkdir` doesn't cover `New-Item`. `git`, `npm`, `cargo` and other programs work the same everywhere.

The matcher is shared: `windows/src-tauri/src/sessions/safe_command_cases.json` runs in `SafeCommandsTests.swift`, `safe_commands.rs` and `newSessionChoices.test.ts`.

## Motion

Pages slide in from the side they lie on. The step pill, tiles, switches and chips spring when they change, and the agent's mark over the prompt lights up once there is something to send. With Reduce motion on, pages only fade and nothing springs.

The pages have no scroll bar. Options keeps worktree and Background or Terminal on one row so the page fits the island. If many custom safe commands make it longer, it scrolls and the top and bottom edges fade out.
