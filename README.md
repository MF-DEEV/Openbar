<p align="center">
  <img src="docs/assets/brand/openbar-hero.gif" width="100%" alt="Openbar, written in violet glass beside a helmet logo, with a beam of light crossing the letters">
</p>

<h3 align="center">Your coding agents, in the notch.</h3>

<p align="center">
  Allow, answer and reply to Claude Code, Codex and four more agents<br>
  from your Mac's Dynamic Island. A full menu bar toolkit comes with it.
</p>

<p align="center">
  <a href="#get-started"><img alt="macOS 14 or later" src="https://img.shields.io/badge/macOS-14%2B-16151d?style=flat-square&logo=apple&logoColor=white"></a>
  <a href="windows/README.md"><img alt="Windows 11 beta" src="https://img.shields.io/badge/Windows%2011-beta-16151d?style=flat-square"></a>
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true"><img alt="Latest Windows release" src="https://img.shields.io/github/v/release/MF-DEEV/Openbar?include_prereleases&filter=windows-*&label=release&style=flat-square&color=8b7cf6&labelColor=16151d"></a>
  <a href="LICENSE"><img alt="License GPL-3.0" src="https://img.shields.io/badge/license-GPL--3.0-8b7cf6?style=flat-square&labelColor=16151d"></a>
</p>

<p align="center">
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=dmg&expanded=true"><b>Download for Mac</b></a>
  &nbsp;&nbsp;&nbsp;
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true"><b>Download for Windows</b></a>
  &nbsp;&nbsp;&nbsp;
  <a href="#documentation">Docs</a>
  &nbsp;&nbsp;&nbsp;
  <a href="https://github.com/MF-DEEV/Openbar/discussions">Discussions</a>
</p>

<br>

<p align="center">
  <img src="docs/assets/readme/sessions/demo.gif" width="820" alt="Five Claude Code sessions in the Dynamic Island: a permission allowed, a question answered, a reply sent, a conversation enlarged, subagents at work and a Clawd dressed up">
</p>

## What it does

Every agent session on your machine sits beside the camera. When one needs you, the island opens with the request in full.

- **Allow** a permission with the whole command or the file diff in view. <kbd>⌘Y</kbd> <kbd>⌘N</kbd> <kbd>⌘T</kbd> work from any app.
- **Answer** a question by picking an option, several, or writing your own.
- **Reply** by typing or speaking, and it lands in the session's own terminal or editor. Speech is recognized on your machine.
- **Watch** a conversation grow, with its tools, tasks, cost and model, and see the subagents a session starts.

| Agent | In the list | Approve in the island | Reply from the island |
|---|:-:|:-:|:-:|
| Claude Code | ✓ | ✓ | ✓ |
| Codex | ✓ | ✓ | ✓ |
| OpenCode | ✓ | ✓ | ✓ |
| Gemini CLI | ✓ | in Gemini CLI | ✓ |
| Copilot CLI | ✓ | in Copilot CLI | ✓ |
| Cursor | ✓ | in Cursor | `cursor-agent` only |

Background sessions (`claude --bg`), plan limits and **New session** are there too. Each session started from New session gets [its own git worktree](docs/session-worktrees.md), so agents working on one repo don't change each other's files. Sessions on [machines you reach over SSH](docs/remote-hosts.md) can show up too, with the host's name on them. See [what each agent allows](docs/more-session-agents.md) and [which terminals replies reach](docs/terminal-hosts.md).

The Mac app also keeps the full menu bar toolkit Openbar grew out of (app switcher, shelf, volume mixer, system monitor, clipboard history and more), and the same island hangs from the top of a Windows 11 screen. [See every feature as a short recording](docs/features.md).

## Get started

**Mac** (Apple silicon, macOS 14 or later):

1. Download the DMG from the [latest Mac release](https://github.com/MF-DEEV/Openbar/releases?q=dmg&expanded=true), open it and drag **Openbar** to **Applications**.
2. This build isn't notarized by Apple yet, so macOS refuses to open it the first time. Go to **System Settings › Privacy & Security**, scroll down and choose **Open Anyway**. Or run this once in Terminal:
   ```sh
   xattr -dr com.apple.quarantine /Applications/Openbar.app
   ```
3. Grant the permissions it asks for (Accessibility and the others each feature needs). Then, in Settings:
   - Install **AI Agents** from the Features hub and turn on **Sessions** under Dynamic Island.
   - Press **Install Hooks** for Claude Code. Your `~/.claude/settings.json` is backed up first, and **Remove Hooks** takes them out. Codex needs nothing.
   - Start `claude` or `codex`. The session appears in the island.

**Windows 11**: run the installer from the [latest Windows release](https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true) (x64 or arm64). It installs for your account only. The installer isn't signed yet, so Windows may say **Windows protected your PC**: choose **More info**, then **Run anyway**. Click the island or press <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>Space</kbd>. The [Windows notes](windows/README.md) cover the rest.

<details>
<summary>Build the Mac app from source</summary>

<br>

You need Apple silicon, macOS 14 or later and the Xcode Command Line Tools.

```sh
git clone https://github.com/MF-DEEV/Openbar.git
cd Openbar
./build.sh --dev --install
```

This builds **Openbar (Developer)**. It has its own bundle ID, so it can sit beside an installed Openbar, but run only one of them at a time or you get two islands. [CONTRIBUTING.md](CONTRIBUTING.md) covers the layout and the checks.

</details>

## Private by default

> [!IMPORTANT]
> Nothing leaves your machine. Prompts, replies and commands stay hidden until you turn on **Show previews**, and even then they're kept in memory only. With the app closed, your agents ask in the terminal as usual. See [everything the app reads and writes](docs/PRIVACY.md).

## Documentation

| Guide | What it covers |
|---|---|
| [Privacy](docs/PRIVACY.md) | What is read, what is kept, what never leaves |
| [Permissions](docs/PERMISSIONS.md) | Every macOS permission, in plain words |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | The common fixes |
| [Claude Code sessions](docs/claude-sessions-spike.md) | What Claude Code exposes and how the island uses it |
| [Codex sessions](docs/codex-sessions.md) | How the island talks to Codex's app server |
| [More agents](docs/more-session-agents.md) | Gemini CLI, Copilot CLI, OpenCode and Cursor |
| [Background sessions](docs/background-sessions.md) | `claude --bg`, Agent View and New session |
| [Session worktrees](docs/session-worktrees.md) | A git worktree for each session started from New session |
| [Terminals](docs/terminal-hosts.md) | Every terminal and editor replies reach |
| [Remote hosts](docs/remote-hosts.md) | Sessions on machines reached over SSH |
| [Features in pictures](docs/features.md) | Every feature as a short recording |
| [Windows](windows/README.md) | The Windows island, installer and build |

## Contributing

Issues and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers the build, the layout and the checks every PR runs. Questions go to [Discussions](https://github.com/MF-DEEV/Openbar/discussions), and security reports follow [SECURITY.md](SECURITY.md). Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).

## License

[GPL 3.0 or later](LICENSE). Openbar is built on [Vorssaint](https://github.com/vorssaint/vorssaint-utils), copyright 2026 Vorssaint; the changes in this repository are copyright 2026 MF DEV, Adrian (@atekuu) and the Openbar contributors, under the same license. Openbar is not affiliated with Vorssaint; see [TRADEMARKS.md](TRADEMARKS.md).

<br>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/logo-dark.svg">
    <img src="docs/assets/readme/logo.svg" width="26" alt="">
  </picture>
  <br>
  <sub>Made by <a href="https://github.com/MF-DEEV">MF DEV</a> and <a href="https://github.com/atekuu">Adrian</a></sub>
</p>
