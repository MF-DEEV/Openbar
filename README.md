<p align="center">
  <img src="docs/assets/brand/openbar-hero.gif" width="100%" alt="Openbar in iron blackletter beside the helmet logo, before a castle at night under a moon that carries the helmet">
</p>

<h3 align="center">Your coding agents, in the notch.</h3>

<p align="center">
  Openbar lists every Claude Code and Codex session beside your Mac's camera.<br>
  When one needs you, you allow it, answer it or reply from there.
</p>

<p align="center">
  <a href="#install"><img alt="macOS 14 or later" src="https://img.shields.io/badge/macOS-14%2B-2A1660?style=flat-square&labelColor=0B0718&logo=apple&logoColor=white"></a>
  <a href="windows/README.md"><img alt="Windows 11" src="https://img.shields.io/badge/Windows%2011-1.0-2A1660?style=flat-square&labelColor=0B0718"></a>
  <a href="windows/docs/linux.md"><img alt="Linux under X11 and Wayland" src="https://img.shields.io/badge/Linux-X11%20%7C%20Wayland-2A1660?style=flat-square&labelColor=0B0718&logo=linux&logoColor=white"></a>
  <a href="LICENSE"><img alt="License GPL-3.0" src="https://img.shields.io/badge/license-GPL--3.0-C8962E?style=flat-square&labelColor=0B0718"></a>
</p>

<p align="center">
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=Mac&expanded=true"><img src="docs/assets/readme/download/mac.png" width="270" alt="Download for Mac, macOS 14 or later"></a>
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true"><img src="docs/assets/readme/download/windows.png" width="270" alt="Download for Windows 11"></a>
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=%22for+Linux%22&expanded=true"><img src="docs/assets/readme/download/linux.png" width="270" alt="Download for Linux: deb, rpm, AppImage"></a>
</p>

<p align="center">
  <a href="docs/README.md">Features and guides</a>
  &nbsp;&nbsp;&nbsp;
  <a href="https://github.com/MF-DEEV/Openbar/discussions">Discussions</a>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/agents/agents-dark.svg">
    <img src="docs/assets/readme/agents/agents-light.svg" width="640" alt="Works with Claude Code, Codex, OpenCode, Gemini CLI, Copilot CLI, Cursor, Pi, Mistral Vibe, Devin CLI, Google Antigravity, Grok CLI, ZCode and Hermes">
  </picture>
</p>

<p align="center">
  <a href="docs/terminal-hosts.md">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/terminals/terminals-dark.svg">
      <img src="docs/assets/readme/terminals/terminals-light.svg" width="640" alt="Open and Reply reach sessions in Terminal, iTerm2, Ghostty, WezTerm, kitty, Windows Terminal, tmux, zellij and the terminals of VS Code, Cursor and Windsurf">
    </picture>
  </a>
</p>

<p align="center">
  <img src="docs/assets/readme/openbar-demo.gif" width="100%" alt="A laptop on a stone table by a campfire with knights around it; the camera moves in to the notch, where Openbar lists Claude Code, Codex, Gemini CLI and OpenCode sessions, and a reply typed in the island lands in the session's terminal">
</p>

<p align="center"><sub>Models: knights by mp4967qwp, JakeHoughton and best3deu, trees by Yag1z and evolveduk, laptop "2021 Macbook Pro 14" (M1 Pro / M1 Max)" by akshatmittal, all on Sketchfab under <a href="https://creativecommons.org/licenses/by/4.0/">CC BY 4.0</a>; scans from Poly Haven (CC0). <a href="docs/assets/brand/source/fantasy/MODELS.md">Sources</a>.</sub></p>

## What Openbar does

- Permission requests, questions and plans open in the island with the full command or diff.
- A reply you type or speak lands in the session's own terminal or editor.
- Claude Code, Codex, OpenCode, Gemini CLI, Copilot CLI, Cursor, Pi, Mistral Vibe, Devin CLI, Google Antigravity, Grok CLI, ZCode and Hermes share one list.
- Each session you start from the island gets its own git worktree.
- Nothing leaves your machine ([privacy](docs/PRIVACY.md)).
- A menu bar toolkit comes with it: a volume mixer, a system monitor, clipboard history and more.

## Install

**Mac** (Apple Silicon, macOS 14 or later), with Homebrew:

```sh
brew install --cask mf-deev/openbar/openbar
```

Or get the DMG from the [latest Mac release](https://github.com/MF-DEEV/Openbar/releases?q=Mac&expanded=true) and drag Openbar to Applications. It isn't notarized yet, so the first time choose **Open Anyway** in System Settings › Privacy & Security.

**Windows 11**, with winget:

```powershell
winget install MFDEV.Openbar
```

Or run the installer from the [latest Windows release](https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true).

On the Mac and on Windows, Openbar then updates itself: it checks once a day and asks before installing. `brew upgrade` and `winget upgrade` work too.

**Linux** (X11, beta): get the `.deb`, `.rpm` or AppImage from the [latest Linux release](https://github.com/MF-DEEV/Openbar/releases?q=%22for+Linux%22&expanded=true), for x86_64 or Arm64. On Wayland the island works on compositors with layer-shell and on GNOME through XWayland, with some features still X11 only; see the [Linux notes](windows/docs/linux.md).

On the Mac, turn sessions on in Openbar's Settings: install **AI Agents** in the Features hub, turn on **Sessions** under Dynamic Island and press **Install Hooks** for Claude Code. Then start `claude` or `codex`.

## Docs

[Features and guides](docs/README.md) · [Privacy](docs/PRIVACY.md) · [Permissions](docs/PERMISSIONS.md) · [Troubleshooting](docs/TROUBLESHOOTING.md) · [Windows](windows/README.md) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) · [Code of conduct](CODE_OF_CONDUCT.md)

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
