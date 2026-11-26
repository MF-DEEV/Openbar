# New session in steps and safe commands (#270)

**+ New session** starts a session in four steps. The steps sit across the top of the page, and a click on any of them goes there.

1. **Project**: where the agent works.
2. **Agent**: Claude Code, Codex or another agent, with its account, model and effort.
3. **Options**: its own worktree, safe commands, and Background or Terminal.
4. **Prompt**: the first thing to say.

Start works from any step. A background session needs a first prompt, so Start goes to step 4 when the prompt is still empty.

## What each project remembers

Each project keeps what it last started with: agent, account, model per agent, effort per agent and model, worktree on or off, Background or Terminal, and its safe commands. A project started before shows a small arrow in the list, and picking it goes straight to the prompt. For a repeat session you pick the project, type the prompt and press Start.

A project with nothing saved starts from the last choices of any project, as New session did before.

- Mac: `notchNewSessionByProject` in the app's defaults, as JSON by folder path (`NewSessionChoices.swift`). Paths belong to this Mac, so the key stays out of settings backups. The last effort of any project is in `notchNewSessionEfforts`.
- Windows and Linux: `openbar.newSession.byProject` in the island's local storage (`newSessionChoices.ts`), the last efforts in `openbar.newSession.efforts`. Windows paths are compared without case.
- The 50 most recently used projects are kept.

## Effort (#289)

Next to Model, **Effort** sets how hard the model thinks, for Claude Code and Codex. It lists only the levels the picked model takes, under the agent's own names, with the agent's own marks: Claude Code's "Recommended" and its usage warning on Max, Codex's "default". The menu's mark is the same set of bars a running session shows.

**Default** comes first and adds nothing to the start, so the agent starts as its settings say. It names the level that gives:

- **Claude Code**: `modelSettings.<model>.effortLevel` in its `settings.json` (what `/effort` saves; Low to Extra, a saved Max is ignored), else the model's own default from its catalog. Claude Code 2.1.318 doesn't start a model at a top-level `effortLevel` in the user's settings, so the island doesn't read one either.
- **Codex**: `model_reasoning_effort` at the top of `config.toml` when the model takes it, else the model's `default_reasoning_level`.

The levels come from the files the model menu already reads: Claude Code's `cache/model-catalog/*-cc.json` (`thinking.effort_options`; `thinking.type: "none"` means none, as for Haiku 4.5) for the account picked, and Codex's `models_cache.json` (`supported_reasoning_levels`). A model with no levels hides the menu, and a remembered level the new model doesn't take (Extra on Opus 4.6) reads as Default.

### How each agent gets it

- **Claude Code**: `--effort <level>` right after `--model`, for `claude --bg` and for a terminal session. It holds for that session and isn't saved.
- **Codex**: a background thread gets `effort` on its first `turn/start` (`thread/start` takes none), and Codex keeps it for the turns after. Codex in a terminal starts with `-c model_reasoning_effort=<level>`, which overrides `config.toml` for that run only. The level is written bare: it isn't TOML, so Codex reads it as the word itself, and no quote has to survive a terminal's or `cmd`'s quoting.
- **Other agents**: no Effort menu yet; what each one takes at start is in [more-session-agents.md](more-session-agents.md#effort-in-new-session-289).

Checked by starting real sessions against a local mock of each API: in Background and Terminal on Windows and Linux, in Background on the Mac (the Mac builds the same arguments, covered by its tests). Claude Code 2.1.318 to 2.1.328 sends the level as `output_config.effort`, and its session log and the island's bars show it; Codex (0.164 and 0.170 in a terminal; the app-server daemon that runs background threads updated itself to 0.172) sends it as `reasoning.effort` (Ultra goes out as `xhigh`, with Codex's own task delegation), and `thread/read` reports it as the thread's `reasoningEffort`.

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
