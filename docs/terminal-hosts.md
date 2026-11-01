# Where Open and Reply reach a session

Issue #121. A session shows up in the island whatever terminal it runs in,
because Claude Code's registry and hooks don't care about the terminal. Open
and Reply have to reach the session's own tab, pane or window, so each
terminal is found and talked to its own way. This page lists them and how
each was checked.

## Mac

The host is read from the session's own environment, innermost terminal
first:

1. `TMUX` and `TMUX_PANE` (tmux). The pane's device is checked against the
   session's before it counts, since an editor opened from tmux keeps those
   variables too.
2. `TERM`. kitty (`xterm-kitty`), Ghostty (`xterm-ghostty`) and Alacritty
   (`alacritty`) set it, and kitty and Alacritty leave a `TERM_PROGRAM` they
   inherited alone.
3. `TERM_PROGRAM`, set by the terminal itself: `Apple_Terminal`,
   `iTerm.app`, `ghostty`, `WezTerm`, `WarpTerminal`, `vscode`. For `vscode`
   the editor comes from the app macOS holds responsible for the process,
   since an editor started from another one's terminal inherits that one's
   bundle identifier.
4. `__CFBundleIdentifier`, which the app that started the shell put there.

| Host | Open | Reply | How it is reached | Needs |
|---|---|---|---|---|
| Terminal | tab | yes | AppleScript, tab by device | Automation |
| iTerm2 | window, tab, split | yes | AppleScript, session by `tty` | Automation |
| Ghostty 1.4+ | window, tab, split | yes | AppleScript, terminal by `tty` (`input text`, `send key "enter"`) | Automation |
| tmux, in any of these | pane, window, and the terminal attached to it | yes | `tmux -S <socket>` of the pane's own server: `paste-buffer -p`, `send-keys Enter`, `select-window`, `select-pane`, `switch-client` | nothing |
| WezTerm | tab and pane, then the window | yes | `wezterm cli` with the pane's socket: `send-text`, `activate-tab`, `activate-pane`, pane checked by `tty_name` | nothing (Accessibility to raise one window of several) |
| kitty, remote control on | window | yes | `kitten @ --to $KITTY_LISTEN_ON`, window checked to list the session's pid | `allow_remote_control` and `listen_on` in kitty.conf |
| kitty, remote control off | app | no, says so | | |
| VS Code, Insiders, Cursor, Windsurf, VSCodium, Antigravity | window holding the folder, then the terminal | yes | the bridge extension (`Tools/AntigravityBridge`), installed per editor from Settings | the extension |
| Warp, Alacritty, anything else | app | no, says so ("Replies aren't available in Warp yet") | | |

The reply is always one bracketed paste per image path and one for the text,
then Return, as `ClaudeTyping.steps` lays out, so line breaks stay text.

## Windows

| Host | Open | Reply | How |
|---|---|---|---|
| Windows Terminal, classic console | tab, window | yes | console attach (`hosts/console.rs`) |
| VS Code, Cursor, Antigravity, Windsurf | terminal | yes | IDE bridge (`hosts/bridge.rs`) |
| WezTerm | tab and pane, then the window | yes | `wezterm.exe` beside the session's `wezterm-gui.exe`, by `WEZTERM_PANE` and `WEZTERM_UNIX_SOCKET`; only those two variables are read from the session's environment block (`process::environment_values`) |
| Git Bash (mintty), Alacritty, Warp, Tabby, Hyper, ConEmu, Rio | window | no, says so | named from the process tree |

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
- **WSL on Windows.** Not answered: the VM has no WSL, and installing it needs
  a reboot and a distribution download. A Claude session inside WSL keeps its
  registry in the Linux home, which the island doesn't read, so it doesn't
  show up today.
- **The VS Code Claude Code panel** (the extension's own chat, not the
  terminal) has no terminal to type into. It is left for a separate issue.

## Not done

- tmux on Windows (it would come through WSL, above).
- Typing into one tab of Warp, Alacritty, mintty: none of them offer a way
  that targets one tab safely.
- The name "tmux in Ghostty" is worked out when the island first sees the
  session; a tmux attached to another terminal later keeps the old name until
  the session restarts. Open always looks again.
