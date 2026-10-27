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
- **Microphone Status.** Muting in Discord shows a red microphone; deafening shows both red headphones and a red microphone, with a short animated notice when muted or unmuted. The unmute notice disappears after three seconds; an active, unmuted microphone has no persistent indicator. Toggle this in **Activity › Microphone Status**. Visible Discord controls in Polish or English provide live mute/deafen state. While Discord is minimized or hidden in the tray, the island follows the mute/deafen shortcuts configured in Discord, including supported keyboard and mouse combinations. It observes those combinations without registering or intercepting them. Returning to Discord synchronizes the state with its controls. The app reads the local keybind configuration without changing it. Browser Discord is not monitored. No audio is recorded and no Discord login or token is needed.
- **Download progress.** New file transfers show a small live indicator. Click it to open a Downloads page with file icons, names and transferred MB; download details never pop up automatically. The app watches the Windows Downloads folder, configured Brave, Chrome, Edge, Vivaldi, Opera and Firefox download folders, plus folders added in **Content › Downloads**. Other programs writing to those folders are supported too. Chromium browsers provide total sizes when available; other transfers show received bytes and an indeterminate indicator. File copies in these folders also appear. Subfolders, protected application caches and downloads outside monitored folders are not scanned. Turn monitoring off in **Activity › Download progress**. Only local metadata and the browser's download records are read; file contents and browsing URLs are not collected.
- **System.** CPU, GPU, memory, battery, network, free disk space, battery power draw and connected USB and Bluetooth devices, as cards that turn orange when something needs attention. Click a card for its recent history and, for CPU and memory, the apps using the most. Readings are taken only while the page is open. Set how often it updates, the network unit and which cards show in **Content › System**.
- **Notifications (requires package identity).** Discord, Messenger and other Windows notifications show the app icon, app name and sender, without message text. Discord's Do Not Disturb and muted chats follow its desktop notifications. See [package identity and setup](docs/notifications.md).
- **Calendar (requires package identity).** The week as a strip, or the month beside your agenda on a tall island, with every appointment from the Windows Calendar app and classic Outlook. In the hour before a meeting the closed island counts down to it (**Content › Calendar › Event countdown**, or right-click one appointment and choose **Add Countdown**). Windows asks once for calendar access. The new Outlook keeps its calendar to itself, so its appointments don't show.
- **Claude Code and Codex sessions.** Every session running on your PC, in one list. Each shows its folder, branch and what it's doing: working, running a tool, waiting for you, or done.
  - **Allow from the island.** On the desktop, a session that needs approval opens with the whole command in view. While another app or a game is in front, it stays in the small waiting-hand indicator without taking focus. Returning to the desktop checks all still-pending requests, including ones that were waiting before the app started. Allow, Deny or Always.
  - **Answer questions.** Pick one of the answers a session offers, or write your own.
  - **Reply without switching apps.** Type in the island and it reaches the session.
  - **Open.** Brings the session's terminal window forward.
  - **A buddy for every session.** Clawd for Claude Code and a little terminal robot for Codex, each in a random outfit you can change.
- **Settings like Windows.** Size, shape, which sections show and in what order, how it opens, which display it uses, starting with Windows and more, in a window that follows your light or dark theme. In **Content**, choose the functions for the lower left and right shortcut circles, or click either highlighted circle in the island preview to change it immediately.

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

- Clipboard, captures, files, tools, camera, scratchpad, AI usage and Watch show "Coming to Windows soon".
- Notifications and Calendar need the signed identity package. Only installers built with it (see below) carry it; without it their pages say so.
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

### The identity package

Notifications and Calendar need a package identity, which a signed sparse package lends `Vorssaint.exe`. Build it once per release on Windows with the Windows SDK; the signing key stays outside the repository:

```powershell
$env:VORSSAINT_SIGNING_PFX = 'C:\keys\vorssaint-signing.pfx'
$env:VORSSAINT_SIGNING_PASSWORD = '…'
.\tools\package-identity\build-sparse.ps1 -NewCertificate   # the first time only; later runs reuse the PFX
```

From the Mac, `tools/package-identity/build-sparse-vm.sh` runs the same script in the build VM and copies the result back. Either way `packaging/out` gets `Vorssaint.msix` and `Vorssaint.cer`, and an installer built with

```sh
bunx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --config src-tauri/tauri.identity.conf.json
```

carries them. See [docs/notifications.md](docs/notifications.md) for what the installer does with them.

For a local update of an existing Windows installation, build with `bun run tauri build --no-bundle`, then run:

```powershell
.\tools\install-local.ps1 -ExecutablePath .\src-tauri\target\release\Vorssaint.exe -EnableAutostart
```

This keeps a backup of the installed executable and preserves settings. It rejects console executables, so a local build cannot accidentally make the island depend on a terminal window. The island uses the Windows GUI subsystem in both release and debug builds; use a full Tauri build for installation so the frontend is bundled and no development server is needed.

Tests run with `bun run test` and, in `src-tauri`, `cargo test`. The design notes behind the port are in [`docs/spec`](docs/spec).

## License

GPL-3.0-or-later, like Vorssaint. See [LICENSE](../LICENSE).
