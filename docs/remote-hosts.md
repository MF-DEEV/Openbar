# Remote hosts over SSH

Issue #189. Claude Code sessions running on another machine show up in the
island next to local ones, with the host's name on them. Permission requests
from them can be allowed or denied from the island, and replies reach them
when they run inside tmux on the host. Codex sessions on the host show up
too (issue #200), with nothing to set up there. This works on the Mac and on
Windows.

## Setting it up

1. Settings → Dynamic Island → Content → Sessions → **Show sessions on remote
   hosts** (on Windows: Settings → Sessions → **Remote hosts**). It is off by
   default.
2. Add a host by its alias from `~/.ssh/config`. Openbar runs the system `ssh`
   (on Windows the OpenSSH client that ships with Windows) with your config,
   keys and agent. It runs with `BatchMode=yes`, so a host that asks for a
   password or an unknown host key fails rather than prompting. Openbar
   never asks for or keeps a password.
3. Once the host shows as connected, press **Set up** and confirm. This adds
   Openbar's hooks to `~/.claude/settings.json` on the host and keeps a copy
   of the file as it was. No other file on the host changes, apart from the
   `~/.openbar` folder that holds the socket.
4. **Remove from host** takes the hooks out again. If nothing else in the
   file changed since setup, the copy goes back byte for byte (or the file is
   deleted if there was none before). If something else changed, only
   Openbar's entries come out. When a Mac and a PC both set up the same
   host, the copy stays until the last one is removed, and that removal puts
   the file back.

## How it works

Every host gets two kinds of `ssh`:

- **A probe**, run at each (re)connect. It prepares `~/.openbar` (mode 700),
  removes a socket left over from the last connection (sshd keeps it unless
  `StreamLocalBindUnlink` is on), and reports the home folder, the system,
  which hook client the host has, whether tmux is there and whether the hooks
  are set up.
- **The link**, which stays open:
  `ssh -T -o BatchMode=yes -o ExitOnForwardFailure=yes -R <host>/.openbar/<app>.sock:<island> <alias> <loader>`.
  The host's socket is forwarded to the island. On the Mac that is the
  island's own hook socket, so a remote hook reaches `ClaudeHookServer`
  exactly like a local one, and permission answers go back the same way.

Every `ssh` runs the same loader, written so that fish, zsh, bash and sh all
read it the same way:

```
exec sh -c 'IFS= read -r s; eval "$(printf %s "$s" | base64 -d)"'
```

Openbar then sends [`Resources/remote-agent.sh`](../Resources/remote-agent.sh)
as one base64 line, followed by a mode. The script is plain POSIX sh (dash,
bash, busybox ash, macOS sh). In the `agent` mode it answers one request per
line, and every answer ends with a lone `.` line. Paths, records and
transcript bytes always travel as base64, so a space or a line break inside
them can't break a field.

| Request | Answer |
|---|---|
| `snap` + `t <offset> <path>`… `end` | `R` per live session (its registry record, config folder, tmux socket, pane and tty), `J` the transcript found for it, `T` transcript bytes after the offset (1 MB at most per look), `G` a transcript that is gone |
| `paste <pid> <session> <text>` | The text into the session's tmux pane as a bracketed paste, after checking that the pane's tty is the session's and that the session isn't waiting on a dialog |
| `enter <pid> <session>` | Return in that pane |
| `focus <pid> <session>` | Selects that window and pane in the host's tmux |

Each `snap` answer also has `X yes` or `X no`: whether Codex's app-server
socket is on the host right now (see [Codex on the host](#codex-on-the-host)).

The island asks for a snapshot every 1.5 seconds. A record counts only while
its process runs and started when the record says: on Linux, `procStart` is
field 22 of `/proc/<pid>/stat` (clock ticks since boot); on a macOS host the
process is checked to be Claude. Remote records join the same engine as local
ones and carry `host = .remote(alias)`. Their process numbers belong to the
host, so none of the local process lookups run on them. Transcript lines are
read with the same reader as local transcripts, so model, tokens, cost, tools
and the chat show up as usual.

### The hooks on the host

Each hook writes to the forwarded socket with whatever the host has. The
first one found is used:

| Client | Command |
|---|---|
| OpenBSD `nc` or nmap `ncat` (both take `-U`) | `'/usr/bin/nc' -U '<socket>'` |
| `socat` | `'/usr/bin/socat' - 'UNIX-CONNECT:<socket>'` |
| `python3` | a few lines with `socket.AF_UNIX` |

Every command ends in `2>/dev/null; exit 0 # openbar-remote-hook <app id>`.
When the link is down the socket refuses at once and Claude Code goes on
asking in its terminal. The marker is different from the local
`# openbar-hook` one, so an Openbar running on the host itself never takes
these entries for its own. The JSON is changed on the island with the same
rules the local installer uses, then written back in one step (temporary
file, then `mv`). If the file's checksum changed since it was read, the write
is skipped and tried again, three times at most.

### Disconnects

When the link ends (network, sleep, sshd restart), the host's sessions leave
the list, its waiting permission hooks are let go (their `ssh` is gone), and
Settings shows `Disconnected:` with ssh's own last line. Openbar tries again
after 2, 4, 8, 16 and then every 30 seconds. Everything runs on the host's own
queue or thread, so a host that hangs never holds up local sessions.

## Windows

Windows uses `%SystemRoot%\System32\OpenSSH\ssh.exe` and the same script.
The difference is the forward. The island's hooks arrive on a named pipe,
which `-R` can't reach, so the host's socket is forwarded to a loopback TCP
port instead (`-R <host socket>:127.0.0.1:<port>`). Any process on the PC
could connect to that port, so each connection is checked with
`GetExtendedTcpTable`: only one whose local end belongs to one of the
island's own `ssh.exe` links is read, and everything else is closed unread.
OpenSSH for Windows 9.5 forwards a remote Unix socket to local TCP both ways.
This was checked in the test VM.

## Codex on the host

Issue #200. Codex needs no hooks: its terminal UIs share one app-server
daemon per user, which listens on
`${CODEX_HOME:-~/.codex}/app-server-control/app-server-control.sock`
(a link into `/tmp/codex-daemon-<uid>/` on Linux). Openbar already joins this
Mac's or this PC's daemon as one more client
([codex-sessions.md](codex-sessions.md)). For a host it joins the host's
daemon the same way, so threads, approvals with their diff, questions,
replies and Always all work as they do locally, and the threads carry the
host's badge. Nothing is installed on the host for this, and Remove from host
doesn't touch it.

1. The probe says where Codex's socket is (`X <path>`), whether or not
   Codex runs yet.
2. Every look at the host says whether the socket is there (`X yes`/`X no`).
3. Each host has its own Codex client. It tries to connect only while the
   host is connected and the last look saw the socket, so a host without
   Codex costs nothing and logs nothing.
4. When the host goes away, the connection to its Codex ends with it: its
   threads leave the list, and the client waits for the host to come back.
   This Mac's or PC's own Codex is a separate client and isn't affected.

How the island reaches the socket differs:

- **Mac:** the link that already runs forwards it the other way:
  `-L ~/.openbar/codex-<hash>.sock:<host socket>`, with
  `StreamLocalBindUnlink=yes` so a socket left from the last connection
  doesn't stop the next one. `<hash>` is the first 6 bytes of a SHA-256 of
  the app's bundle ID and the alias, so the name is short enough for a
  socket and differs between hosts and between the Developer and release
  builds. `ssh` makes the socket with mode 600 in the app's 700 folder, and
  the Codex client checks its peer is this same user, as it does locally.
  A host path with a `:` in it can't be forwarded this way and is skipped.
- **Windows:** the OpenSSH that ships with Windows (9.5) can't listen on a
  local Unix socket (`-L` says `Operation not supported`), and `-W` to a
  remote Unix socket hangs. Forwarding to a loopback TCP port would work,
  but any process on the PC could then drive the host's Codex, and the
  island can't check who connects to a port `ssh` owns. So the island runs
  a second `ssh` per host while Codex runs there, in the agent script's
  `codex` mode, and talks WebSocket over its standard input and output. On
  the host, that mode joins them to the socket with what the host has:
  `socat`, OpenBSD `nc -N -U` (`-N` ends the connection when the island's
  side does; macOS `nc` has a `-N` of its own and runs without it), or a few
  lines of `python3`. A host with none of these shows its Claude Code
  sessions but not its Codex ones. No port listens on the PC.

Images can't be attached to a host's sessions, Codex's included: the paths
are on this machine. Open has no window to bring forward for a host's Codex
thread, since its terminal is on the host. New session starts Codex on this
machine only.

## Replies

- **Inside tmux on the host:** pasted with `tmux load-buffer` and
  `paste-buffer -p -r -d`, then `send-keys Enter`, over the link. The pane is
  found from `TMUX` and `TMUX_PANE` in the session's `/proc/<pid>/environ`
  (Linux hosts).
- **Anywhere else:** the reply field says replies over SSH reach sessions
  inside tmux only. Permissions still work, because they go through the hooks.
- **Images** aren't sent to remote sessions: their paths are on this machine.

## What was checked, and how

The test host was a Debian 13 arm64 cloud image in QEMU on the Mac, reached
as `openbar-linux`. It ran the real Claude Code 2.1.319 inside tmux, pointed
at a local stand-in for the Messages API (`ANTHROPIC_BASE_URL` and
`apiKeyHelper`), which asks to run one Bash command. That gives a real
permission prompt without an account or tokens.

On the Mac, with the Developer build:

- Set up asked first, then added 11 hooks and kept the copy.
- The session showed up within a couple of seconds, with an `openbar-linux`
  badge, model, tokens and cost read from the transcript.
- Allow in the island: the remote TUI said `Allowed by PermissionRequest
  hook`, the command ran, and the turn finished.
- A reply typed in the island arrived in the tmux pane and Claude answered it.
- Stopping sshd on the host: Settings said `Disconnected: …`, the remote row
  left, local sessions stayed as they were, and the host came back on its own
  once sshd was up.
- Remove from host left `~/.claude/settings.json` byte for byte as it was at
  setup, and removed `~/.openbar`.

The same flow was then run from the Windows 11 ARM64 VM against the same
host: Set up, the badge, Allow, a reply into the tmux pane, a disconnect and
recovery, and Remove from host. That VM's clock was an hour behind the
host's, which is how the clock correction below was found. Last, both
islands set the host up at once (22 hooks). A permission reached both, the
first answer won, and removing both left the file byte for byte as it was.

### Codex on the host (issue #200)

The same Debian VM ran the packaged Codex 0.164 for Linux
(`codex-package-aarch64-unknown-linux-musl`; the bare binary refuses to start
the daemon) in tmux, with its daemon on. A `model_providers` entry pointed it
at a local stand-in for the Responses API that asks for a command outside
the project, a file outside the project, or a question (the question needs
Codex's Plan mode). No account and no tokens were used. On the Mac, with the
Developer build, and on Windows 11 ARM64 in the VM, the island showed the
thread with the `openbar-linux` badge and:

- Allow: the command ran on the host. Allow on a file change showed the
  diff first and wrote the file. Deny on a file change wrote nothing (Mac).
- Always answered the request waiting at the time.
- A question's answer (`hotfix`) reached Codex.
- A reply typed in the island started a new turn there.
- Stopping sshd on the host took the thread off the list and left local
  sessions alone; when sshd came back, the thread came back on its own, with
  the approval it was still waiting on.

On Windows the bridge on the host was `socat`; `nc -N -U` and the `python3`
fallback were checked by hand, and the Mac and Rust tests run the script's
`codex` mode against a socket of their own.

### Clocks

The engine drops a permission whose hook came before the registry's last
change. The registry's times are on the host's clock, so with each snapshot
the agent sends `N <host time in ms>`. Records and transcript timestamps are
then moved by the offset, measured against the middle of the request.

## Not done (follow-ups)

- tmux detection on macOS hosts (no `/proc`): permissions work there, replies
  don't yet.
- Subagent transcripts and task lists of remote sessions. Subagents still
  show from their hooks.
- Hosts whose Claude Code keeps its settings under `CLAUDE_CONFIG_DIR`: setup
  writes `~/.claude/settings.json` only (records from other `~/.claude*`
  folders are listed).
