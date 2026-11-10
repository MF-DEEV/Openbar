<p align="center">
  <img src="docs/assets/brand/openbar-hero.gif" width="100%" alt="Openbar, written in violet glass beside a helmet logo, with a beam of light crossing the letters">
</p>

<h3 align="center">Your coding agents, in the notch.</h3>

<p align="center">
  Openbar lists every Claude Code and Codex session beside your Mac's camera.<br>
  When one needs you, you allow it, answer it or reply from there.
</p>

<p align="center">
  <a href="#install"><img alt="macOS 14 or later" src="https://img.shields.io/badge/macOS-14%2B-16151d?style=flat-square&logo=apple&logoColor=white"></a>
  <a href="windows/README.md"><img alt="Windows 11 beta" src="https://img.shields.io/badge/Windows%2011-beta-16151d?style=flat-square"></a>
  <a href="windows/docs/linux.md"><img alt="Linux under X11" src="https://img.shields.io/badge/Linux-X11-16151d?style=flat-square&logo=linux&logoColor=white"></a>
  <a href="LICENSE"><img alt="License GPL-3.0" src="https://img.shields.io/badge/license-GPL--3.0-8b7cf6?style=flat-square&labelColor=16151d"></a>
</p>

<p align="center">
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=Mac&expanded=true"><b>Download for Mac</b></a>
  &nbsp;&nbsp;&nbsp;
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true"><b>Download for Windows</b></a>
  &nbsp;&nbsp;&nbsp;
  <a href="docs/README.md">Features and guides</a>
  &nbsp;&nbsp;&nbsp;
  <a href="https://github.com/MF-DEEV/Openbar/discussions">Discussions</a>
</p>

https://github.com/user-attachments/assets/e6782d65-7822-4d9a-9ad4-3f31c8c35df4

<p align="center"><sub>Three agents on one repo, each in its own git worktree. The edit skips the waiting.<br>Laptop model: "2021 Macbook Pro 14" (M1 Pro / M1 Max)" by akshatmittal on Sketchfab, <a href="https://creativecommons.org/licenses/by/4.0/">CC BY 4.0</a>.</sub></p>

## What it does

- A permission request, a question or a plan opens in the island with the whole command or the file diff. You allow, deny or answer it without leaving the app you're in.
- A reply you type or speak in the island lands in the session's own terminal or editor.
- Claude Code, Codex, OpenCode, Gemini CLI, Copilot CLI and Cursor share one list, along with their subagents and background sessions.
- A session you start from the island gets its own git worktree, so agents on one repo don't change each other's files.
- Nothing leaves your machine. [Privacy](docs/PRIVACY.md) lists everything the app reads and writes.
- The menu bar toolkit Openbar grew out of is still here: a volume mixer, a system monitor, clipboard history, a window switcher and more.

[Features and guides](docs/README.md) shows all of it, with recordings.

## Install

**Mac**: Apple Silicon and macOS 14 or later. Download the DMG from the [latest Mac release](https://github.com/MF-DEEV/Openbar/releases?q=Mac&expanded=true) and drag Openbar to Applications. The build isn't notarized yet, so the first time choose **Open Anyway** in System Settings › Privacy & Security. Then, in Openbar's Settings:

1. Install **AI Agents** from the Features hub and turn on **Sessions** under Dynamic Island.
2. Press **Install Hooks** for Claude Code. Your `~/.claude/settings.json` is backed up first. Codex needs nothing.
3. Start `claude` or `codex`. The session appears in the island.

**Windows 11**: run the installer from the [latest Windows release](https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true). It's a beta for x64 and arm64, and it installs for your account only.

**Linux**: X11 only, built from source. See the [Linux notes](windows/docs/linux.md).

To build the Mac app yourself, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Docs

[Features and guides](docs/README.md) · [Privacy](docs/PRIVACY.md) · [Permissions](docs/PERMISSIONS.md) · [Troubleshooting](docs/TROUBLESHOOTING.md) · [Windows](windows/README.md)

Issues and pull requests are welcome, and [CONTRIBUTING.md](CONTRIBUTING.md) covers the build and the checks every PR runs. Questions go to [Discussions](https://github.com/MF-DEEV/Openbar/discussions), security reports follow [SECURITY.md](SECURITY.md), and everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).

## License

[GPL 3.0 or later](LICENSE). Openbar is built on [Vorssaint](https://github.com/vorssaint/vorssaint-utils), copyright 2026 Vorssaint; the changes in this repository are copyright 2026 MF DEV, Adrian (@atekuu) and the Openbar contributors, under the same license. Openbar is not affiliated with Vorssaint; see [TRADEMARKS.md](TRADEMARKS.md).

## Contributors

<div align="center">
  <table>
    <tr>
      <td align="center" width="150"><a href="https://github.com/MF-DEEV"><img src="https://github.com/MF-DEEV.png?size=160" width="72" height="72" alt=""><br><b>MF DEV</b></a></td>
      <td align="center" width="150"><a href="https://github.com/atekuu"><img src="https://github.com/atekuu.png?size=160" width="72" height="72" alt=""><br><b>Adrian</b></a></td>
      <td align="center" width="150"><a href="https://github.com/Dotims"><img src="https://github.com/Dotims.png?size=160" width="72" height="72" alt=""><br><b>Dotims</b></a></td>
    </tr>
  </table>
</div>
