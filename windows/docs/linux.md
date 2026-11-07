# Linux (X11)

The island runs on Linux under X11 (tested on XFCE with xfwm4 and picom). Wayland is not supported yet.

## Build

You need [Bun](https://bun.sh), [Rust](https://rustup.rs) and the Tauri system libraries (`webkit2gtk-4.1`, `gtk3`, `libayatana-appindicator`, `librsvg`, `openssl`, `xdotool`). To run it, sound also needs `pactl` and `parec` (the `libpulse` package on Arch and Fedora, `pulseaudio-utils` on Debian and Ubuntu, with PulseAudio 16 or later or PipeWire's pulse server). Then, in `windows`:

```sh
bun install --frozen-lockfile
bun run build
cd src-tauri
cargo build --features tauri/custom-protocol   # embeds ../dist; without it the app loads the dev server
```

Packages: `bun run tauri build --bundles deb,rpm,appimage` (the config's own target list is the Windows installer).

## What works

- The island window: transparent, always on top, a dock window that takes no focus. The window is cut to the island's shape (XShape), so clicks anywhere else go to the windows below, which also works under xfwm4. A compositor that blurs behind windows (picom) blurs the whole cut, so the cut follows what is drawn, not where the island is going, and is exactly the painted shape (a notch's ears included, with no margin), or the blur would show as a frosted outline around the island; the round buttons grow in without fading.
- Claude Code sessions, read from `~/.claude/sessions` through `/proc`. Hooks reach the island over a socket in `~/.openbar` (in the temp folder when the home folder's path is too long for a socket address).
- Open and Reply (tmux, kitty and WezTerm are covered by tests against stand-in programs, not run against the real ones): Open raises the terminal's window through the window manager (`_NET_ACTIVE_WINDOW`). Replies go into tmux (`$TMUX`, `$TMUX_PANE`; the pane must be on the session's own terminal, since an editor opened from a pane keeps both), kitty (needs `allow_remote_control` and `listen_on`; the window must list the session's process), WezTerm (`wezterm cli`) and VS Code, Cursor, Antigravity and Windsurf through the bridge extension. Any other terminal is raised, not typed into.
- The island hides while a fullscreen app is active (`_NET_WM_STATE_FULLSCREEN`), and "All displays" puts a mirror on each other monitor.
- Now playing (`media.rs`): any player on the session bus that speaks MPRIS (Spotify, VLC, Rhythmbox, a browser tab, ...), with its cover, seek, shuffle, the source menu and Open player (raises the player, or starts it from the applications menu). The Spotify queue is read through Windows accessibility and has no Linux counterpart. Covers come from files and HTTPS, through the web view's own HTTP library (libsoup).
- Sound (`audio/`): the Mixer and Controls over the sound server (PulseAudio, or PipeWire's pulse server) through `pactl`: the output's and microphone's level and mute, the default device, every app that plays with its own level, Mic Mute, audio priority, the headphone guard, the output switcher and the finer volume keys. An app counts as playing while its stream is not paused. The Music page's player volume and the spectrum next to the song work too; the spectrum records the player's own stream with `parec`. The microphone indicator shows when an app records from a microphone (it reads the server's lists and never records, so it has no level meter).
- Icons and names (`shell.rs`): the icon theme and the desktop entries give app icons, file icons and thumbnails, and the system monitor opens from the System page.
- Power (`battery.rs`, `power_saver.rs`, `session_lock.rs`, `keep_awake.rs`): the machine's own batteries from `/sys/class/power_supply` (charge, charger, rate, time left, wear and cycles; a mouse's battery or a UPS is left out), the energy saver as power-profiles-daemon's `power-saver` profile, the unlock of the session (from systemd-logind, a screensaver service, or a screen locker program such as `xsecurelock` or `i3lock` that was running and ended), and Keep Awake through logind's inhibitor and the desktop's power manager.
- Brightness (`brightness.rs`): a panel's backlight, written through logind, and external displays over DDC/CI with `ddcutil` (which needs the `i2c` group).
- Quick toggles (`toggles.rs`): lock the screen, turn the display off (X11 power management), start the screensaver, empty the trash, and dark mode on GNOME-family desktops. Hidden files, desktop icons and ejecting disks differ too much between desktops and are not offered.
- Notifications (`notifications.rs`): there is no list of other apps' notifications to read on Linux, so the island listens to the calls apps make to the notification service (a monitor connection to the session bus, as `dbus-monitor` does; the desktop's own popup still shows). It keeps the title, text and icon of what arrives while it runs, folds an app's updates of one notification into one entry, and leaves out transient ones and the volume and brightness displays. Open starts the app the notification came from.
- System (`system_monitor.rs`): CPU, memory (with swap, cache and zram), network, disk space and activity, the GPU (`nvidia-smi`, or an AMD card's busy file), battery, uptime and connected USB and Bluetooth devices, read from `/proc`, `/sys` and BlueZ, with the CPU, memory and GPU (NVIDIA) process lists grouped by program. Network traffic and power by process need privileges or a trace, so those lists say they are unavailable.
- Global shortcuts, tray icon (its menu; a plain click does not open Settings, the tray protocol has no click event), launch at login (`~/.config/autostart/openbar.desktop`).
- Settings and caches live in `~/.config/com.openbar.island` and `~/.cache/com.openbar.island`.

## Where the code is

Everything Linux does differently is in `src-tauri/src/linux/`, a file for each piece of Windows code it stands in for: the window (`island.rs`), fullscreen (`fullscreen.rs`), launch at login (`autostart.rs`), processes from `/proc` (`process.rs`), terminals and editors (`hosts.rs`, `tmux.rs`, `kitty.rs`, `wezterm.rs`, `x11.rs`, `command.rs`, `ide.rs`), session records (`sessions.rs`), hooks (`hooks.rs`), the XDG folders (`paths.rs`), now playing (`media.rs`), power and brightness (`battery.rs`, `power_saver.rs`, `session_lock.rs`, `keep_awake.rs`, `brightness.rs`, `toggles.rs`), notifications (`notifications.rs`), the System page (`system_monitor.rs`), sound (`audio.rs` and `pulse.rs`, with `microphone.rs` and `music_levels.rs` for the indicator and the spectrum) and icons (`shell.rs`, `image.rs`, `desktop.rs`). The Windows modules only hand over to these with `cfg(target_os = "linux")`, and the code Windows compiles is otherwise as it was. A new Linux piece is a new file there and one hand-over where the Windows module has its own.

## NVIDIA

WebKitGTK cannot back its GBM buffers on the proprietary driver and leaves every page blank. The app sets `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` when `/proc/driver/nvidia/version` exists, unless one of the `WEBKIT_DMABUF_RENDERER_*` or `WEBKIT_DISABLE_DMABUF_RENDERER` variables is already set. `WEBKIT_DISABLE_DMABUF_RENDERER=1` alone does not draw the island's body.

## Not done yet

- Wayland (needs a layer-shell window).
- "Show in screenshots" and background sessions.
- The toolkit sections that read Windows APIs (clipboard, calendar, screenshots, ...) are not ported.
