<h1 align="center">Vorssaint for Windows</h1>

<p align="center">
  The Dynamic Island from Vorssaint for Mac, at the top of a Windows 11 screen.<br>
  Your music, your notifications and every Claude Code and Codex session, one click away.
</p>

<p align="center">
  <a href="https://github.com/MF-DEEV/vorssaint-utils/releases?q=windows&expanded=true"><b>Download</b></a> ·
  <a href="#install">Install</a> ·
  <a href="#what-it-does">What it does</a> ·
  <a href="#private-by-default">Privacy</a> ·
  <a href="#build-it-yourself">Build it yourself</a>
</p>

<p align="center">
  <img src="docs/assets/island-sessions.png" width="640" alt="The island open on its Sessions page: Claude Code and Codex sessions in one list, one waiting for permission with Allow and Deny, one asking a question, others working or done">
</p>

<p align="center"><sub>The Sessions page, shown with demo sessions.</sub></p>

This is a port of the Dynamic Island from the unofficial [Vorssaint](https://github.com/vorssaint/vorssaint-utils) fork in this repository. It is not affiliated with or endorsed by the Vorssaint maintainer. It is a beta: the sections listed under [Not yet](#not-yet) show a placeholder for now.

## Install

1. Download the installer for your PC from the [latest Windows release](https://github.com/MF-DEEV/vorssaint-utils/releases?q=windows&expanded=true):
   - `Vorssaint_<version>_x64-setup.exe` for most PCs (Intel and AMD).
   - `Vorssaint_<version>_arm64-setup.exe` for PCs with a Snapdragon or other ARM processor.
2. Run it. The installer isn't signed yet, so Windows may say **Windows protected your PC**. Choose **More info**, then **Run anyway**.
3. Go through the setup. It installs for your account only, needs no administrator password, and starts the island when it's done.

You need Windows 11. The island sits at the top center of your main display. Click it to open, or press **Ctrl+Alt+Space** from any app.

To remove it, use **Settings › Apps › Installed apps › Vorssaint › Uninstall**. That also takes out its Start with Windows entry and the hooks it added to Claude Code.

## What it does

- **A notch at the top of the screen.** A black island hangs from the top edge, like the camera housing on a MacBook, or floats as a capsule if you prefer. It grows into pages when you click it and steps aside for games and films in full screen.
- **Now Playing.** Whatever plays in Spotify, a browser or Media Player shows beside the camera with its cover and moving bars. Open it to see the song, scrub, shuffle and control playback. Media keys keep working as usual.
- **Notifications.** New Windows notifications appear as a banner in the island and stay on its page until you dismiss them.
- **Claude Code and Codex sessions.** Every session running on your PC, in one list. Each shows its folder, branch and what it's doing: working, running a tool, waiting for you, or done.
  - **Allow from the island.** When a session asks to run a command, the island opens on its own with the whole command in view. Allow, Deny or Always.
  - **Answer questions.** Pick one of the answers a session offers, or write your own.
  - **Reply without switching apps.** Type in the island and it reaches the session.
  - **Open.** Brings the session's terminal window forward.
  - **A buddy for every session.** Clawd for Claude Code and a little terminal robot for Codex, each in a random outfit you can change.
- **Settings like Windows.** Size, shape, which sections show and in what order, how it opens, which display it uses, starting with Windows and more, in a window that follows your light or dark theme.

### Using it with Claude Code

Open the island's settings with the gear beside it, go to **Content › Sessions** and choose **Install hooks**. Vorssaint adds its hooks to `~/.claude/settings.json`, next to your own and with a backup first. Without them the island still lists your sessions, but it can't answer their permission prompts.

### Using it with Codex

Nothing to install. While Codex runs, the island talks to the app server Codex starts for its terminal sessions, so approvals and replies go to Codex itself.

## Private by default

- Nothing the island reads leaves your PC. There is no telemetry, no analytics and no account.
- Prompts, replies, session titles and command output stay hidden unless you turn on **Show previews** in **Content › Sessions**. With it on, they are kept in memory only and cleared when you turn it off.
- A permission prompt always shows what it would run, so nothing is allowed unread.
- Your settings live in `%APPDATA%\com.vorssaint.island\settings.json` and only hold what you changed.

## Not yet

- Timer, calendar, clipboard, captures, files, system, tools, camera, downloads, scratchpad, AI usage and Watch show "Coming to Windows soon".
- Mirroring Windows notifications needs a signed package, which this installer doesn't carry yet. Its page says so.
- There are no automatic updates. To update, run the newer installer over the old one; your settings stay.
- The settings window is in English.
- Codex sessions are tested end to end on Windows. Claude Code sessions use the same island, but their hooks and replies typed into Windows Terminal haven't been tested end to end on Windows yet.

## Build it yourself

You need [Bun](https://bun.sh), [Rust](https://rustup.rs) and, on Windows, the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
cd windows
bun install
bun run tauri build
```

The installer lands in `src-tauri/target/release/bundle/nsis/`. The installers in the releases are cross-compiled on a Mac with `cargo-xwin` and `makensis`:

```sh
bunx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
bunx tauri build --runner cargo-xwin --target aarch64-pc-windows-msvc
```

Tests run with `bun run test` and, in `src-tauri`, `cargo test`. The design notes behind the port are in [`docs/spec`](docs/spec).

## License

GPL-3.0-or-later, like Vorssaint. See [LICENSE](../LICENSE).
