# Where Open and Reply reach a session

Issue #121. A session shows up in the island whatever terminal it runs in,
because Claude Code's registry and hooks don't care about the terminal. Open
and Reply have to reach the session's own tab, pane or window, so each
terminal is found and talked to its own way. This page lists them and how
each was checked.

## Mac

The host is read from the session's own environment, innermost terminal
first:

1. `ZELLIJ`, `ZELLIJ_SESSION_NAME` and `ZELLIJ_PANE_ID` (zellij, #235). They
   count only when a zellij server is an ancestor of the session: zellij
   starts each pane's program itself, while an editor, or a tmux, started
   from a pane runs apart from it. zellij leaves the outer terminal's
   `TERM_PROGRAM` alone, so its variables are read first.
2. `TMUX` and `TMUX_PANE` (tmux). The pane's device is checked against the
   session's before it counts, since an editor opened from tmux keeps those
   variables too.
3. `TERM`. kitty (`xterm-kitty`), Ghostty (`xterm-ghostty`) and Alacritty
   (`alacritty`) set it, and kitty and Alacritty leave a `TERM_PROGRAM` they
   inherited alone.
4. `TERM_PROGRAM`, set by the terminal itself: `Apple_Terminal`,
   `iTerm.app`, `ghostty`, `WezTerm`, `WarpTerminal`, `vscode`. For `vscode`
   the editor comes from the app macOS holds responsible for the process,
   since an editor started from another one's terminal inherits that one's
   bundle identifier.
5. `__CFBundleIdentifier`, which the app that started the shell put there.

| Host | Open | Reply | How it is reached | Needs |
|---|---|---|---|---|
| Terminal | tab | yes | AppleScript, tab by device | Automation |
| iTerm2 | window, tab, split | yes | AppleScript, session by `tty` | Automation |
| Ghostty 1.4+ | window, tab, split | yes | AppleScript, terminal by `tty` (`input text`, `send key "enter"`) | Automation |
| tmux on a remote host over SSH ([remote hosts](remote-hosts.md)) | pane and window in the host's tmux | yes | the host's `tmux` through the island's `ssh` link: `paste-buffer -p`, `send-keys Enter` | the host set up in Settings |
| tmux, in any of these | pane, window, and the terminal attached to it | yes | `tmux -S <socket>` of the pane's own server: `paste-buffer -p`, `send-keys Enter`, `select-window`, `select-pane`, `switch-client` | nothing |
| zellij 0.44+, in any of these | tab and pane, and the terminal attached to it | yes | `zellij --session <name> action` with the server's own binary: `paste --pane-id`, `send-keys --pane-id … Enter`, `go-to-tab-by-id`, `focus-pane-id` | nothing |
| WezTerm | tab and pane, then the window | yes | `wezterm cli` with the pane's socket: `send-text`, `activate-tab`, `activate-pane`, pane checked by `tty_name` | nothing (Accessibility to raise one window of several) |
| kitty, remote control on | window | yes | `kitten @ --to $KITTY_LISTEN_ON`, window checked to list the session's pid | `allow_remote_control` and `listen_on` in kitty.conf |
| kitty, remote control off | app | no, says so | | |
| VS Code, Insiders, Cursor, Windsurf, VSCodium, Antigravity | window holding the folder, then the terminal | yes | the bridge extension (`Tools/AntigravityBridge`), installed per editor from Settings | the extension |
| Warp, Alacritty, anything else | app | no, says so ("Replies aren't available in Warp yet") | | |

The reply is always one bracketed paste per image path and one for the text,
then Return, as `ClaudeTyping.steps` lays out, so line breaks stay text.

A tmux or zellij can be nested in the other. Open goes from the innermost
pane outward: the multiplexer selects the pane, then the terminal its client
runs in is brought forward the way a session in it would be, a pane of the
outer multiplexer included.

### No terminal attached (#235)

When the session's tmux or zellij, or a multiplexer around it, has no client
at all, Open says so on its button ("Detached · Attach") for ten seconds.
Pressing it runs `zellij attach <name>` or `tmux -S <socket> attach-session -t
<session>` in a new window of the terminal the person's sessions use. On
Windows and Linux the service works that line out again when Attach is
pressed; the page never sends one. Replies still reach a detached pane.

### zellij, how it is reached

- Every `zellij action` from outside a session goes to the client that was
  used last (typed into last), like tmux's most active client.
  `focus-pane-id` only works once a client has had input since it attached;
  until then Open still changes the tab. It exits with 2 when the pane is
  already focused, which counts as done.
- The pane's tab (`tab_id` from `list-panes --json`) is selected first with
  `go-to-tab-by-id`, which works on a client that hasn't been used yet.
- `list-clients` names each client's focused pane and nothing else: no
  process, no device. The client's process is found by its unix socket: on
  the Mac `lsof -U` gives the peer address of each `zellij` process's
  socket, on Linux `ss -xn` the peer inode, and a client is the process whose
  socket's peer is one of the server's. Among several, the one whose device
  was read last (its access time, which input updates) is the one used last.
  Windows has no way to see a socket's peer from outside, so a client counts
  there when its command line names the session (`attach <name>`, `-s
  <name>`) or when it started the server (a plain `zellij`).
- zellij names each server's socket with a hash
  (`$TMPDIR/zellij-<uid>/contract_version_1/<hash>`), not the session, so the
  session comes from the pane's environment. Renaming a session leaves the old
  name there; Open and Reply then find nothing.
- Client and server have to be the same version, so the server's own binary
  (`proc_pidpath`, `/proc/<pid>/exe`, the process image) runs every action.

## Windows

| Host | Open | Reply | How |
|---|---|---|---|
| Windows Terminal, classic console | tab, window | yes | console attach (`hosts/console.rs`) |
| VS Code, Cursor, Antigravity, Windsurf | terminal | yes | IDE bridge (`hosts/bridge.rs`) |
| zellij 0.44+ (native; zellij runs on Windows since 0.44) | tab and pane, then the console its client runs in | yes | `zellij.exe --session <name> action`, as on the Mac (`hosts/zellij.rs`) |
| WezTerm | tab and pane, then the window | yes | `wezterm.exe` beside the session's `wezterm-gui.exe`, by `WEZTERM_PANE` and `WEZTERM_UNIX_SOCKET`; only those two variables are read from the session's environment block (`process::environment_values`) |
| Git Bash (mintty), Alacritty, Warp, Tabby, Hyper, ConEmu, Rio | window | no, says so | named from the process tree |

## Which tab is in front (#210)

The sound when a session waits stays quiet when the session's own tab or pane is in front and on screen. Each host is asked as below; anything else, a slow answer (over 1.5 s) or an error plays the sound.

| Host | How it tells |
|---|---|
| Terminal, iTerm2, Ghostty 1.4+ (Mac) | the app is frontmost with its window the front ordinary window on screen, then AppleScript gives the device of the front window's selected tab (`selected tab`, `current session`, `focused terminal of selected tab`); never asks for Automation |
| WezTerm | `wezterm cli list-clients`: the focused pane of the client used last, with WezTerm's window in front |
| kitty, remote control on | `kitten @ ls`: `is_focused` on the OS window, the tab and the window |
| tmux | `#{pane_active}#{window_active}` is `11`, then the terminal of the client used last on that session is asked the same (Linux: kitty, WezTerm, or a tmux or zellij around it) |
| zellij | every attached client has the pane focused (`list-clients`), then the terminal of the client used last is asked the same; none has it: hidden; only some: can't tell |
| Classic console (Windows) | its window is the foreground window and not minimised |
| Windows Terminal | its window is in front, and the selected tab (UI Automation) bears the console's title (`GetConsoleTitleW`) and no other tab does |
| Editors, Warp, Alacritty, GNOME Terminal, Konsole, remote and background sessions | can't tell: the sound plays |

## What was checked, and how

A stand-in session (a Node program that registers itself in
`~/.claude*/sessions` like Claude Code does, asks for bracketed paste and logs
every byte it gets) ran in each terminal, so a reply's exact bytes could be
compared without spending tokens.

- Mac, through the installed app's island (Accessibility presses, nothing
  typed by hand): Reply to iTerm2 3.7.6, Ghostty 1.4.0, tmux 3.8 inside
  Terminal, WezTerm and kitty 0.49.2 arrived as `ESC[200~text ESC[201~` then
  `\r`. Open picked the session's tab over another session's in iTerm2,
  Ghostty, WezTerm and kitty, and the tmux window and the Terminal tab
  attached to it in tmux. A kitty without remote control shows "Turn on
  remote control in kitty.conf to reply".
- Mac, the extension: Install in Settings put it into VS Code 1.144, and the
  app updated it in VS Code and Antigravity by itself when its version rose.
  VS Code's extension host answers the bridge's ping. A reply into a real VS
  Code terminal was not run.
- Windows 11 ARM64 VM, WezTerm 20240203 (x64, software WebGPU): the live test
  `hosts::tests::live_session_reply_reaches_its_host` found pane 0 from the
  stand-in's environment, the reply arrived with its line break and quotes,
  and Open switched to that pane's tab from another. Git Bash was checked in
  the browser build of the island only.

## Questions the issue asked first

- **tmux.** Reproduced before the change: a process in a tmux pane started
  from Terminal inherits `__CFBundleIdentifier=com.apple.Terminal` and has the
  pane's device (`/dev/ttys029`), which no Terminal tab has. It was classified
  as a Terminal tab, so Reply failed and Open only raised Terminal. tmux sets
  `TMUX`, `TMUX_PANE` and `TERM_PROGRAM=tmux` in every pane.
- **Ghostty.** Ghostty 1.3 added AppleScript; 1.4 gives each terminal a `tty`
  and a `pid`, which is what makes a safe match possible. On 1.3 asking for
  `tty` fails and the session counts as not found: Ghostty comes forward and
  nothing is typed.
- **Warp.** No scripting that can target one tab; it is raised and says so.
- **WSL on Windows.** A Claude session inside WSL keeps its registry in the
  Linux home, which the island doesn't read, and writes a Linux `pidDomain`
  whose pids aren't Windows pids (`sessions/registry.rs` leaves it out on
  purpose), so it doesn't show up at all, and neither does a zellij or tmux
  there. zellij inside WSL is out of reach for the same reason; zellij run
  natively on Windows (0.44 and later) is reached as above.
- **The VS Code Claude Code panel** (the extension's own chat, not the
  terminal) has no terminal to type into. It is left for a separate issue.

## Not done

- tmux on Windows (it would come through WSL, above).
- zellij on remote hosts: remote sessions are reached through tmux only.
- Typing into one tab of Warp, Alacritty, mintty: none of them offer a way
  that targets one tab safely.
- The name "tmux in Ghostty" is worked out when the island first sees the
  session; a tmux attached to another terminal later keeps the old name until
  the session restarts. Open always looks again.
