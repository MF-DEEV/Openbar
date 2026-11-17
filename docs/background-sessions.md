# Background sessions

Claude Code can run a whole session with no terminal (`claude --bg`) and lists them in Agent View (`claude agents`). The island shows these sessions, acts on them and can start new ones. This is not the same as a background shell or subagent inside a normal session (`phase == .background`).

Code: `ClaudeBackgroundSessions.swift` on the Mac, `windows/src-tauri/src/sessions/background.rs` on Windows.

## What Claude Code does (checked on 2.1.316, macOS)

| Thing | Finding |
|---|---|
| Registry | `sessions/<pid>.json` has `kind: "bg"` and `jobId`. `claude agents --json` says `kind: "background"` for the same session. A session's short id starts its session id. |
| Processes | A per-user `claude daemon` runs each session in its own pty. The process has a controlling terminal but no `__CFBundleIdentifier`, so it is told apart by `kind`, never by its host. |
| Transcript | The usual `projects/<folder>/<session>.jsonl`, so the rest of Sessions works unchanged. |
| States | `claude agents --json --all` gives `working`, `blocked`, `done`, `failed`, `stopped`. `blocked` with registry status `idle` means it waits for a reply with no dialog open, as after `claude --bg` with no prompt. |
| Ended | After `claude stop` the job has no `pid` and no registry record. `claude attach` brings it back. `claude logs` fails: the daemon goes away with its last worker. |
| Hooks | `SessionStart`, `PermissionRequest`, `Notification` and `Stop` fire as usual, and an `allow` from the hook runs the tool. |
| Permission mode | Without `--permission-mode` a session takes the person's default, which may be `auto`: then no prompt appears at all. |
| Trust | `--bg` in a folder never trusted fails with `Workspace not trusted`. Only a terminal can ask, so the island says so and offers Terminal. It never writes the trust flag itself. |
| Turned off | `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` makes both `agents --json` and `--bg` exit 1. `disableAgentView` in settings is read by the island too. Passing it through `--settings` did not stop `--bg`, so the island checks the setting files itself. |
| `claude logs` | The last ~20 KB of the raw terminal stream, with cursor moves. A session animating a spinner fills it with that alone. |

## Replies

The peer socket (`messagingSocketPath`) labels text as not from the user, so it is not used. Agent View's own reply box exists only inside its TUI, and no input command exists.

The island runs `claude attach <id>` in a pseudo terminal nobody sees (`openpty` on the Mac, ConPTY on Windows), pastes the text, presses Return and leaves with Ctrl+Z, all documented behavior. Checked on the Mac: the message lands in the transcript as a plain user message (`userType: external`) with its line breaks, the session answers it, and another terminal attached at the same time stays attached. As with terminal replies, nothing is typed while a dialog is open, since the keys would answer it.

## Logs

`claude logs`, read through a small screen emulator. Where that leaves fewer than three lines with words in them, the island attaches for a moment instead and reads the full screen Claude Code paints for whoever attaches.

## Environment

A GUI app starts with almost no `PATH`, and the daemon keeps the environment it is started with for every session it runs. So on the Mac commands run with the environment of a login shell (`zsh -ilc 'env -0'`, once), minus anything marking a Claude session. Commands that act on a session in a moved config folder pass `CLAUDE_CONFIG_DIR`.

## Windows

The official docs mention attaching on Windows (a `←` detach detail), and the binary carries ConPTY handling, but **no real `claude --bg` has run on Windows yet**: the test VM has no Claude Code. Everything was driven in the VM against a test double of the CLI (`claude.exe` reading its console in VT mode). That proves the app's side: the commands and their arguments, the registry reading, ConPTY attach, paste bytes, Ctrl+Z, a new console opening in Windows Terminal, and the whole UI. To check on a real install:

- [ ] What `pidDomain` a Windows record has (`registry::WINDOWS_PID_DOMAINS` guesses `win32`).
- [ ] That Ctrl+Z through ConPTY detaches. If it doesn't, the attach is ended after 3 s, which the CLI documents as harmless.
- [ ] That a reply reaches the session as the person's own, as on the Mac.
