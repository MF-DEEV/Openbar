# Linux (X11)

The island runs on Linux under X11 (tested on XFCE with xfwm4 and picom). Wayland is not supported yet.

## Build

You need [Bun](https://bun.sh), [Rust](https://rustup.rs) and the Tauri system libraries (`webkit2gtk-4.1`, `gtk3`, `libayatana-appindicator`, `librsvg`, `openssl`, `xdotool`). Then, in `windows`:

```sh
bun install --frozen-lockfile
bun run build
cd src-tauri
cargo build --features tauri/custom-protocol   # embeds ../dist; without it the app loads the dev server
```

Packages: `bun run tauri build --bundles deb,rpm,appimage` (the config's own target list is the Windows installer).

## What works

- The island window: transparent, always on top, a dock window that takes no focus. The window is cut to the island's shape (XShape), so clicks anywhere else go to the windows below, which also works under xfwm4.
- Claude Code sessions, read from `~/.claude/sessions` through `/proc`. Hooks reach the island over a socket in `~/.openbar` (in the temp folder when the home folder's path is too long for a socket address).
- Open and Reply (tmux, kitty and WezTerm are covered by tests against stand-in programs, not run against the real ones): Open raises the terminal's window through the window manager (`_NET_ACTIVE_WINDOW`). Replies go into tmux (`$TMUX`, `$TMUX_PANE`; the pane must be on the session's own terminal, since an editor opened from a pane keeps both), kitty (needs `allow_remote_control` and `listen_on`; the window must list the session's process), WezTerm (`wezterm cli`) and VS Code, Cursor, Antigravity and Windsurf through the bridge extension. Any other terminal is raised, not typed into.
- The island hides while a fullscreen app is active (`_NET_WM_STATE_FULLSCREEN`), and "All displays" puts a mirror on each other monitor.
- Global shortcuts, tray icon (its menu; a plain click does not open Settings, the tray protocol has no click event), launch at login (`~/.config/autostart/openbar.desktop`).
- Settings and caches live in `~/.config/com.openbar.island` and `~/.cache/com.openbar.island`.

## NVIDIA

WebKitGTK cannot back its GBM buffers on the proprietary driver and leaves every page blank. The app sets `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` when `/proc/driver/nvidia/version` exists, unless one of the `WEBKIT_DMABUF_RENDERER_*` or `WEBKIT_DISABLE_DMABUF_RENDERER` variables is already set. `WEBKIT_DISABLE_DMABUF_RENDERER=1` alone does not draw the island's body.

## Not done yet

- Wayland (needs a layer-shell window).
- "Show in screenshots" and background sessions.
- The toolkit sections that read Windows APIs (media, notifications, battery, ...) are not ported.
