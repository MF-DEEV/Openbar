# Where Open and Reply reach a session

Issue #121. A session shows up in the island whatever terminal it runs in,
because Claude Code's registry and hooks don't care about the terminal. Open
and Reply have to reach the session's own tab, pane or window, so each
terminal is found and talked to its own way. This page lists them and how
each was checked.

## Mac

The host is read from the session's own environment, innermost terminal
first:

1. `HERDR_ENV`, `HERDR_PANE_ID`, `HERDR_SOCKET_PATH` and `HERDR_SESSION`
   (Herdr, #328). They count only when a Herdr server (`herdr server`) is an
   ancestor of the session with no zellij or tmux server between: Herdr
   starts each pane's program itself, while a tmux or zellij started in a
   pane runs its own server, whose sessions are that one's. Herdr sets
   `TERM_PROGRAM=herdr` over the outer terminal's.
2. `ZELLIJ`, `ZELLIJ_SESSION_NAME` and `ZELLIJ_PANE_ID` (zellij, #235). They
   count only when a zellij server is an ancestor of the session: zellij
   starts each pane's program itself, while an editor, or a tmux, started
   from a pane runs apart from it. zellij leaves the outer terminal's
   `TERM_PROGRAM` alone, so its variables are read first.
3. `TMUX` and `TMUX_PANE` (tmux). The pane's device is checked against the
   session's before it counts, since an editor opened from tmux keeps those
   variables too.
4. `TERM`. kitty (`xterm-kitty`), Ghostty (`xterm-ghostty`) and Alacritty
   (`alacritty`) set it, and kitty and Alacritty leave a `TERM_PROGRAM` they
   inherited alone.
5. `TERMINAL_EMULATOR=JetBrains-JediTerm`, set by a JetBrains IDE's
   terminal (#329), when no terminal named itself in `TERM_PROGRAM`. The IDE
   is the app macOS holds responsible for the process, as for `vscode`
   below, so an editor or terminal started from an IDE's terminal is told
   apart from it.
6. `TERM_PROGRAM`, set by the terminal itself: `Apple_Terminal`,
   `iTerm.app`, `ghostty`, `WezTerm`, `WarpTerminal`, `vscode`. For `vscode`
   the editor comes from the app macOS holds responsible for the process,
   since an editor started from another one's terminal inherits that one's
   bundle identifier.
7. `__CFBundleIdentifier`, which the app that started the shell put there. A
   JetBrains IDE's identifier counts too (any app with a
   `Contents/Resources/product-info.json`).

| Host | Open | Reply | How it is reached | Needs |
|---|---|---|---|---|
| Terminal | tab | yes | AppleScript, tab by device | Automation |
| iTerm2 | window, tab, split | yes | AppleScript, session by `tty` | Automation |
| Ghostty 1.4+ | window, tab, split | yes | AppleScript, terminal by `tty` (`input text`, `send key "enter"`) | Automation |
| tmux on a remote host over SSH ([remote hosts](remote-hosts.md)) | pane and window in the host's tmux | yes | the host's `tmux` through the island's `ssh` link: `paste-buffer -p`, `send-keys Enter` | the host set up in Settings |
| tmux, in any of these | pane, window, and the terminal attached to it | yes | `tmux -S <socket>` of the pane's own server: `paste-buffer -p`, `send-keys Enter`, `select-window`, `select-pane`, `switch-client` | nothing |
| zellij 0.44+, in any of these | tab and pane, and the terminal attached to it | yes | `zellij --session <name> action` with the server's own binary: `paste --pane-id`, `send-keys --pane-id … Enter`, `go-to-tab-by-id`, `focus-pane-id` | nothing |
| Herdr 0.9+, in any of these | workspace, tab and pane, and the terminal attached to it | yes | Herdr's socket API on the pane's `HERDR_SOCKET_PATH`: `pane.send_input` (a paste, bracketed when the pane asks for it), `pane.send_keys`, `pane.focus` | nothing |
| WezTerm | tab and pane, then the window | yes | `wezterm cli` with the pane's socket: `send-text`, `activate-tab`, `activate-pane`, pane checked by `tty_name` | nothing (Accessibility to raise one window of several) |
| kitty, remote control on | window | yes | `kitten @ --to $KITTY_LISTEN_ON`, window checked to list the session's pid | `allow_remote_control` and `listen_on` in kitty.conf |
| kitty, remote control off | app | no, says so | | |
| VS Code, Insiders, Cursor, Windsurf, VSCodium, Antigravity | window holding the folder, then the terminal | yes | the bridge extension (`Tools/AntigravityBridge`), installed per editor from Settings | the extension |
| JetBrains IDEs (IntelliJ IDEA, PyCharm, WebStorm, GoLand, PhpStorm, RubyMine, CLion, Rider, RustRover, Android Studio …), with the bridge plugin | project window, then the terminal tab | yes | the bridge plugin (`Tools/JetBrainsBridge`) in the IDE, on `~/.openbar/terminals/<IDE pid>.sock` | the plugin, installed per IDE from Settings, and a restart of the IDE |
| JetBrains IDEs without the plugin | project window | no: "Install the JetBrains bridge to reply", with a way to Settings | the project Claude Code's JetBrains plugin lists as open in that IDE, opened again through the IDE's launcher (`<IDE>.app/Contents/MacOS/<launcher> <folder>`), which brings its window forward (`open -a` doesn't); else the window whose title names the session's folder (Accessibility) | nothing |
| Warp, Alacritty, anything else | app | no, says so ("Replies aren't available in Warp yet") | | |

The reply is always one bracketed paste per image path and one for the text,
then Return, as `ClaudeTyping.steps` lays out, so line breaks stay text.

tmux, zellij and Herdr can be nested in one another. Open goes from the
innermost pane outward: the multiplexer selects the pane, then the terminal
its client runs in is brought forward the way a session in it would be, a
pane of the outer multiplexer included.

### No terminal attached (#235)

When the session's tmux, zellij or Herdr, or a multiplexer around it, has no
client at all, Open says so on its button ("Detached · Attach") for ten
seconds. Pressing it runs `zellij attach <name>`, `tmux -S <socket>
attach-session -t <session>`, or `herdr session attach <name>` (plain `herdr`
for Herdr's default session) in a new window of the terminal the person's
sessions use. On
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

### Herdr, how it is reached (#328)

Herdr ([herdrdev/herdr](https://github.com/herdrdev/herdr), herdr.dev,
0.9.5 when this was written) is a multiplexer made for coding agents. Its own
CLI and agents talk to its server over a socket API, one JSON request per
line (`{"id":…,"method":"pane.focus","params":{"pane_id":"w1:p2"}}`) and one
answer per line; Openbar speaks it directly, so no `herdr` binary has to be
found for Open or Reply.

- Every pane's environment holds `HERDR_ENV=1`, the pane (`HERDR_PANE_ID`,
  `w1:p2`), its workspace and tab, the server's API socket
  (`HERDR_SOCKET_PATH`, `~/.config/herdr/sessions/<name>/herdr.sock`, or
  `~/.config/herdr/herdr.sock` for the default session) and, for a named
  session, `HERDR_SESSION`. Openbar talks only to a socket of the person's
  own (checked with `lstat`), never to a path the environment points at
  otherwise.
- Pane ids are never given again once a pane closes, and a moved pane's old
  id still finds it.
- **Reply:** `pane.send_input` with the text, which Herdr wraps in bracketed
  paste when the pane asked for it, then `pane.send_keys` `enter`. Neither
  moves the focus.
- **Open:** `pane.get` checks the pane is there, `pane.focus` switches to its
  workspace and tab and focuses it. Herdr keeps one focus for the whole
  session, which every attached terminal shows. The terminal that comes
  forward is the one of the client used last, found as for zellij: the
  client is a `herdr` process on a terminal whose unix socket's peer is one
  of the server's sockets other than the API socket (a `herdr pane …` run
  from some terminal talks over that one and is no client), `lsof -U` on the
  Mac, `ss -xn` and `/proc/<pid>/fd` on Linux.
- **Which pane is in front (#210):** `session.snapshot`'s
  `focused_pane_id` is the pane and a terminal is attached, then that
  terminal is asked as below.
- The server starts as a child of the first client and stays when the client
  goes (`herdr server`, or `herdr server --handoff-import …` after an update).
- **Herdr's own agent states** (idle, working, blocked, done) come from its
  screen-reading detection or its own hooks. Openbar doesn't read them: the
  island already has each session's state from the agent's own hooks and
  records, which say more (the tool, the question, the plan).
- **Windows.** Herdr also runs natively on Windows (x64, in beta), where
  its socket API is a named pipe. Openbar doesn't reach Herdr on Windows
  yet.

## Windows

| Host | Open | Reply | How |
|---|---|---|---|
| Windows Terminal, classic console | tab, window | yes | console attach (`hosts/console.rs`) |
| VS Code, Cursor, Antigravity, Windsurf | terminal | yes | IDE bridge (`hosts/bridge.rs`) |
| JetBrains IDEs, with the bridge plugin | project window, then the terminal tab | yes | the plugin, registered in `%LOCALAPPDATA%\Openbar\terminals\<IDE pid>.json` with a loopback port and a key (`hosts/jetbrains.rs`, `hosts/bridge.rs`) |
| JetBrains IDEs without it | project window | no, says so | the project Claude Code's lock file lists, given to the IDE's launcher (`bin\idea64.exe <folder>`), else the IDE's window whose title names the folder |
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
| Herdr | Herdr's one focus (`session.snapshot`) is the pane, then the terminal of the client used last is asked the same; no terminal attached: hidden |
| Classic console (Windows) | its window is the foreground window and not minimised |
| Windows Terminal | its window is in front, and the selected tab (UI Automation) bears the console's title (`GetConsoleTitleW`) and no other tab does |
| JetBrains IDEs, with the bridge plugin | the IDE is in front, and the plugin says the session's project window is the active one, its terminal tool window is open and the session's tab is the one selected there |
| Editors, JetBrains IDEs without the plugin, Warp, Alacritty, GNOME Terminal, Konsole, remote and background sessions | can't tell: the sound plays |

## JetBrains IDEs (#329)

JetBrains has no outside scripting for its terminal, so Openbar carries a
small plugin for every IDE on the IntelliJ Platform, 2025.1 and later
(`Tools/JetBrainsBridge`). Settings › Sessions lists the IDEs it finds,
each with Install, Update and Remove; the IDE loads or drops the plugin
when it restarts. Openbar updates a plugin it installed when a newer one
ships, and takes it out of every IDE when Openbar is uninstalled.

- **Finding the IDEs.** Every IDE on the platform carries a
  `product-info.json` (in `Contents/Resources` on the Mac, beside `bin`
  elsewhere) naming the product, its settings folder (`PyCharm2026.3`) and
  its launcher. The plugin goes into `~/Library/Application
  Support/JetBrains/<folder>/plugins` on the Mac,
  `%APPDATA%\JetBrains\<folder>\plugins` on Windows and
  `~/.local/share/JetBrains/<folder>` on Linux (`Google` instead of
  `JetBrains` for Android Studio). The Mac asks LaunchServices and looks in
  `/Applications`, `~/Applications` and the folder Toolbox used before 2.0;
  Windows in `Program Files\JetBrains`, `%LOCALAPPDATA%\Programs` and
  Toolbox's apps folder; Linux in `/opt`, `/snap`, `~/.local/share/JetBrains/Toolbox/apps`
  and a few more. Gateway and the remote development client are left out.
- **Telling a session in one.** On the Mac, `TERMINAL_EMULATOR` and the app
  responsible for the process, then the IDE process found above the session.
  On Windows and Linux, the IDE's launcher among the session's ancestors
  (`pycharm64.exe`, `pycharm`), or on Linux a `java` whose runtime sits in an
  IDE's install; the IDE's own name comes from its `product-info.json`.
- **The plugin.** One process serves all the IDE's project windows. It
  answers the requests the VS Code bridge answers (`ping`, `focus`, `send`,
  `send-images`, `steps`) and one more, `shown`, for the waiting sound. A
  request names the session's shell, or all its ancestors on Windows and
  Linux, and only the IDE holding that shell's tab says yes. It finds the
  tab by the pid of its shell, in the classic terminal (JediTerm, whose pty
  it writes to) and in the reworked terminal of 2025.3 and later (whose
  classes it reaches through the loader of an open tab, since a plugin
  outside JetBrains can't name that module). It listens on
  `~/.openbar/terminals/<pid>.sock` on the Mac, a folder only its owner can
  open; on Linux on a socket in the runtime folder, and on Windows on a port
  of the loopback address, each with a random key in
  `Openbar/terminals/<pid>.json` that every request has to bring. Nothing is
  read from the terminals or the projects.
- **Open.** The IDE comes forward first (on Windows the island lets it,
  `AllowSetForegroundWindow`), then the plugin raises the tab's project
  window, opens the terminal tool window and selects the tab. Without the
  plugin, a project folder is only ever given to the IDE when Claude Code's
  JetBrains plugin lists it as open in that very IDE process, so Open never
  opens a project or a window; otherwise the IDE's window whose title names
  the session's folder comes forward.
- **Claude Code's lock file.** `<config>/ide/<port>.lock` also holds the
  token of Claude's own connection to the IDE. Openbar reads the file
  through a type with no field for it (`ClaudeIDELockFile`,
  `jetbrains::ClaudeLock`), so it is never kept or used, and tests check
  that. Folders written WSL-style on Windows (`/mnt/c/work/app`) are read as
  `C:\work\app`.

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
- JetBrains (#329), without a JetBrains IDE running: the IDEs' first start
  asks to accept the JetBrains User Agreement, which is the person's to
  accept. A stand-in IDE ran the plugin's own server and protocol classes
  with "tabs" that are stand-in sessions on a pty each, written to the way
  the plugin writes to a pty. On the Mac it was an app with a
  `product-info.json`, started through LaunchServices from a VS Code
  terminal (so its sessions inherited `TERM_PROGRAM=vscode`); the island's
  own code (a probe built from the app's sources, and the menu bar panel of
  an isolated test copy) named the IDE, found its process and socket, typed a
  two-line reply as one bracketed paste and a return, pressed keys, and Open
  selected the right one of three tabs. With the socket gone the reply field
  asked for the bridge, and Open gave the IDE the project Claude Code's lock
  file lists. Settings listed the real IntelliJ IDEA, PyCharm and WebStorm
  2026.3 installs and put the plugin into and out of PyCharm's plugins
  folder. On Linux (the arm64 VM) the stand-in ran as `bin/pycharm` of an IDE
  folder, like the native launcher; `hosts::locate` named it PyCharm, Open
  and a reply with the registration's key reached the right one of two tabs.
  The plugin's tests run with JDK 17 on the Mac and Linux.
- JetBrains (#329) in real IDEs on the Mac: IntelliJ IDEA 2026.3 with two
  projects (three terminal tabs in one, one in the other) and PyCharm 2026.3
  with a third, stand-in sessions in the tabs. Settings installed the plugin
  into IntelliJ IDEA, PyCharm and WebStorm, updated WebStorm's outdated copy
  and removed it again; after a restart IntelliJ IDEA and PyCharm logged the
  plugin listening. Open (from the menu bar panel and from a probe) raised
  the right project window, across the two IDEs too, and selected the right
  tab, in the reworked terminal and, after switching IntelliJ IDEA to the
  classic one, there too; `shown` said yes for that tab only, and no while
  another app was in front. Replies reached a tab that wasn't selected, as
  one bracketed paste and a return, and keys arrived. Without the plugin the
  reply field asked for it and Open raised the window by its title, or gave
  the IDE the folder from Claude Code's lock file. That run found two bugs,
  both fixed: `open -a <IDE> <folder>` doesn't bring an open project
  forward (the IDE's launcher does), and the "Restart to load it" note
  stayed after the restart.

- Herdr (#328), 0.9.5 from Homebrew on the Mac and the release binary on
  Linux (arm64 VM, Xvfb with xfwm4, kitty), with stand-in sessions in three
  panes of one Herdr session: one beside a shell, one in a second tab and one
  in a second workspace. On the Mac, through an isolated test copy's island
  (Accessibility presses): each row carried a "Herdr" chip and "Open in Herdr
  in Terminal"; Open from another app switched Herdr to the session's
  workspace, tab and pane and brought the Terminal window of the Herdr client
  used last forward; a two-line reply reached its pane only, as one
  bracketed paste and a return, while Herdr's focus stayed on another pane.
  A probe built from the app's sources said "shown" for the focused pane
  with its Terminal window in front, and "hidden" for the other panes, with
  another app in front, and with another Terminal window in front. The
  waiting sound stayed quiet for the focused pane and played for another.
  With the client closed, Open said "Herdr · detached" and Attach opened
  `herdr session attach <name>` in a new Terminal window. On Linux the live
  test `hosts::tests::live_session_reply_reaches_its_host` did the same
  (Open from another window raised kitty on the right pane, a reply reached
  only that pane, `shown` true and false as above, `herdr session attach`
  for a session with no client). Nested both ways on the Mac and on Linux: a
  session in tmux or zellij run in a Herdr pane is tmux's or zellij's, and
  Open brings Herdr's pane forward around it; a session in Herdr run in a
  tmux pane is Herdr's, and Open selects the tmux pane too.

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
- Herdr on Windows (above), and Herdr on remote hosts (`herdr --remote`,
  saved machines): a session there is reached like any other remote one.
- Typing into one tab of Warp, Alacritty, mintty: none of them offer a way
  that targets one tab safely.
- JetBrains on Windows and Linux with real IDEs (stand-ins and unit tests
  only), and WebStorm with the plugin loaded (its first start waited on
  macOS's "downloaded from the internet" prompt).
- IntelliJ IDEA 2026.3 warns "Cannot update plugin: unable to identify where
  Openbar Terminal Bridge was installed from" for any plugin put in its
  plugins folder rather than installed from a repository. Openbar keeps the
  plugin up to date itself; the warning would go if the plugin were on
  JetBrains Marketplace. Claiming Marketplace as its source without being
  there would let whoever takes the plugin's id there update it.
- The name "tmux in Ghostty" is worked out when the island first sees the
  session; a tmux attached to another terminal later keeps the old name until
  the session restarts. Open always looks again.
