<h1 align="center">Claude sessions in your notch</h1>

<p align="center">
  Every Claude Code session on your Mac, in the Dynamic Island.<br>
  Allow, answer and reply without switching to the terminal.
</p>

<p align="center">
  <a href="#what-it-does">What it does</a> ·
  <a href="#set-it-up">Set it up</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#private-by-default">Privacy</a> ·
  <a href="#on-windows">Windows</a> ·
  <a href="#about-this-fork">About this fork</a>
</p>

<p align="center">
  <img src="docs/assets/readme/claude-sessions.gif" width="720" alt="Five Claude Code sessions working in the Dynamic Island, then a permission allowed, a question answered, a reply sent, a conversation enlarged, subagents at work and a Clawd getting a top hat">
</p>


This is an unofficial fork of [Vorssaint](https://github.com/vorssaint/vorssaint-utils) that adds a Sessions section to its Dynamic Island. It is not affiliated with or endorsed by the Vorssaint maintainer. Everything in the video is the real app driving real Claude Code sessions, recorded on a MacBook.

## What it does

- **Every session at a glance.** Running sessions are listed beside the camera with their name, project, git branch and state: working, running a tool, waiting for permission, waiting for you, done or idle. Each project gets its own color, and a session that sat idle for a while fades out.
- **Allow from the island.** When Claude asks to run something, the island opens on its own with the whole command in view once you are at the desktop or in Vorssaint; while you work in another app, the request waits as a count on the closed island instead. Allow, Deny or Always, by click or with ⌘Y, ⌘N and ⌘T, which work from any app while the request is on screen.
- **Answer Claude's questions.** Questions Claude asks with options show up as a card. Pick one, pick several, or write your own answer.
- **Reply without switching apps.** Type a reply in the island and it reaches the session in Terminal or Antigravity as if typed there. ⌘↩ sends, and ⌘V or a drop adds images.
- **Watch it work.** Enlarge a session's page to read the whole conversation as it grows, with recent tools, tasks, cost and the model beside it, or fold the details away.
- **Subagents too.** Agents a session starts get rows of their own, including the ones sent to the background, colored like their agent definitions.
- **Codex too.** Codex CLI sessions join the same list, under Codex's own mark and color, with a little terminal robot instead of Clawd. Its approvals, questions and replies go through Codex's own server, so there is nothing to install. Always on a Codex request allows every request of the task under way, with Always: ON on its page and Cancel to stop it, until the task ends.
- **Background sessions too.** Sessions started with `claude --bg` are listed with a Background mark and Agent View's name. Answer their prompts, reply, read their output, stop or remove them, or open a terminal on one. **New session** starts Claude Code in a recent folder, in the background or in a terminal.
- **A Clawd for every session.** Each session and subagent gets a pixel Clawd in a random costume. Open the wardrobe to change its shell, hat, eyewear and extras, or let Surprise me pick.

## Set it up

You need a Mac with Apple Silicon, macOS 14 or newer, the Xcode Command Line Tools and [Claude Code](https://docs.anthropic.com/en/docs/claude-code).

1. Build and install it:

   ```sh
   git clone https://github.com/MF-DEEV/vorssaint-utils.git
   cd vorssaint-utils
   ./build.sh --dev --install
   ```

   This installs a separate Developer build next to any official Vorssaint. Run only one of them at a time, or you get two islands.

2. In Settings, install the AI Agents feature from the Features hub, then turn on the Sessions section in the Dynamic Island settings.
3. For Claude Code, press **Install Hooks** in the Sessions settings. Hooks let the island show which tool runs and answer permission prompts and questions. They are added to `~/.claude/settings.json` next to your own, after a backup, and **Remove Hooks** takes them out again. Codex needs nothing: its sessions show up while it runs.
4. If you work in Antigravity, press **Install Extension** so replies can reach its terminals. For Terminal, macOS asks for Automation access the first time you send a reply.
5. Start `claude` in Terminal or Antigravity. The session shows up in the island.

## How it works

The island reads the small records Claude Code keeps for each running session in `~/.claude/sessions`, then follows that session's transcript, subagents and tasks as they grow. Permission prompts and questions arrive through Claude Code's own hooks, which talk to the app over a local socket. Claude Code shows the same prompt in the terminal at the same time, and whichever answer comes first wins. With the app closed, Claude Code simply asks in the terminal as usual.

Replies are typed into the session's own Terminal tab, found by its terminal device, or into its Antigravity terminal through the extension.

Codex sessions come from the app server Codex's terminal UIs share, at `~/.codex/app-server-control/app-server-control.sock`. The island joins a thread while it works or while its page is open, shows its approvals and questions, and sends your answers and replies back through that server. [The Codex notes](docs/codex-sessions.md) describe the protocol.

## Private by default

Nothing leaves your Mac. Prompts, replies, titles and commands stay unread until you turn on **Show previews**, which is off by default, and even then they are kept in memory only and cleared when you turn it off. The [privacy notes](docs/PRIVACY.md) list everything the Sessions page reads and writes, and the [permissions guide](docs/PERMISSIONS.md) covers the Automation access replies use.

## On Windows

The Dynamic Island also runs on Windows 11, with the same sections: Sessions for Claude Code and Codex, Controls, the volume mixer, Now Playing, clipboard history, the Files shelf, System, Tools, Calendar, notifications, Timer, Downloads, AI Agents and Watch. It is a beta with its own installer: download it from the [Windows releases](https://github.com/MF-DEEV/vorssaint-utils/releases?q=windows&expanded=true) (x64 for most PCs, arm64 for Snapdragon), and read [windows/README.md](windows/README.md) for what it does and how to build it.

## About this fork

The rest of the app is Vorssaint as it is upstream: volume mixer, system monitor, app switcher, window snapping, clipboard history, the Dynamic Island and much more. Its [README](https://github.com/vorssaint/vorssaint-utils#readme) describes all of it, and official builds come only from the [Vorssaint releases](https://github.com/vorssaint/vorssaint-utils/releases).

On the Mac this fork ships no builds of its own; you build it from source as above. The Windows island has installers in this repository's releases. The Vorssaint name, icon and look belong to its maintainer, as [TRADEMARKS.md](TRADEMARKS.md) explains.

- [Privacy](docs/PRIVACY.md), what does and does not leave your Mac
- [Permissions](docs/PERMISSIONS.md), every macOS permission in plain words
- [Troubleshooting](docs/TROUBLESHOOTING.md), the common fixes
- [Contributing](CONTRIBUTING.md), build, layout and conventions
- [Claude Code session notes](docs/claude-sessions-spike.md), what Claude Code exposes and how the island uses it
- [Codex session notes](docs/codex-sessions.md), how the island talks to Codex's app server

### Checks on pull requests

Every pull request runs [PR checks](.github/workflows/pr.yml) for the parts it changes, each job capped at fifteen minutes, and **CI ok** sums them up as the one check to require before merging into `main`. You can run the same checks before you push:

- Mac: `./build.sh --test`
- Windows UI, in `windows/`: `bun run typecheck`, `bun run format:check`, `bun run test`
- Windows backend, in `windows/src-tauri/`, after `bun run build` in `windows/`: `cargo fmt --check`, `cargo xwin clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings` and `cargo test`. CI pins Rust 1.99.1 (`RUST_TOOLCHAIN` in the workflow), so a newer local toolchain can report lints CI does not.

The release build and its selftest are not part of CI; run `./build.sh && ./build/Vorssaint --selftest` yourself for runtime, UI or bundled resource changes.

## License

[GPL 3.0 or later](LICENSE). Vorssaint is copyright 2026 Vorssaint; the changes in this fork are copyright 2026 MF DEV under the same license.

<p align="center">
  <sub>Fork by <a href="https://github.com/MF-DEEV">MF DEV</a> · Based on <a href="https://github.com/vorssaint/vorssaint-utils">Vorssaint</a></sub>
</p>
