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
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=Mac&expanded=true"><img src="docs/assets/readme/download/mac.png" width="270" alt="Download for Mac: Apple Silicon, macOS 14 or later"></a>
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true"><img src="docs/assets/readme/download/windows.png" width="270" alt="Download for Windows 11, x64 and Arm64"></a>
  <a href="https://github.com/MF-DEEV/Openbar/releases?q=linux&expanded=true"><img src="docs/assets/readme/download/linux.png" width="270" alt="Download for Linux: X11, deb, rpm and AppImage"></a>
</p>

<p align="center">
  <a href="docs/README.md">Features and guides</a>
  &nbsp;&nbsp;&nbsp;
  <a href="https://github.com/MF-DEEV/Openbar/discussions">Discussions</a>
</p>

https://github.com/user-attachments/assets/e6782d65-7822-4d9a-9ad4-3f31c8c35df4

<p align="center"><sub>Laptop model: "2021 Macbook Pro 14" (M1 Pro / M1 Max)" by akshatmittal on Sketchfab, <a href="https://creativecommons.org/licenses/by/4.0/">CC BY 4.0</a>.</sub></p>

## What it does

- Permission requests, questions and plans open in the island with the full command or diff.
- A reply you type or speak lands in the session's own terminal or editor.
- Claude Code, Codex, OpenCode, Gemini CLI, Copilot CLI and Cursor share one list.
- Each session you start from the island gets its own git worktree.
- Nothing leaves your machine ([privacy](docs/PRIVACY.md)).
- A menu bar toolkit comes with it: a volume mixer, a system monitor, clipboard history and more.

## Install

**Mac** (Apple Silicon, macOS 14 or later): get the DMG from the [latest Mac release](https://github.com/MF-DEEV/Openbar/releases?q=Mac&expanded=true) and drag Openbar to Applications. It isn't notarized yet, so the first time choose **Open Anyway** in System Settings › Privacy & Security.

**Windows 11** (beta): run the installer from the [latest Windows release](https://github.com/MF-DEEV/Openbar/releases?q=windows&expanded=true).

**Linux** (X11, beta): get the `.deb`, `.rpm` or AppImage from the [latest Linux release](https://github.com/MF-DEEV/Openbar/releases?q=linux&expanded=true), for x86_64 or Arm64. Wayland isn't supported yet; see the [Linux notes](windows/docs/linux.md).

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
