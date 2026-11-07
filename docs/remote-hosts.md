# Remote hosts over SSH

Issue #189. Claude Code sessions running on another machine show up in the
island next to local ones, with the host's name on them. Permission requests
from them can be allowed or denied from the island, and replies reach them
when they run inside tmux on the host. This works on the Mac and on Windows.

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
   Openbar's entries come out.

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
host.

## Not done (follow-ups)

- **Codex on remote hosts.** Codex's app server listens on a Unix socket
  (`~/.codex/app-server-control/app-server-control.sock`). It could be
  forwarded the other way (`-L <local socket>:<that socket>`) and joined by
  the existing Codex client, which would give threads, approvals and replies
  with no hooks at all. It isn't in this change.
- tmux detection on macOS hosts (no `/proc`): permissions work there, replies
  don't yet.
- Subagent transcripts and task lists of remote sessions. Subagents still
  show from their hooks.
- Hosts whose Claude Code keeps its settings under `CLAUDE_CONFIG_DIR`: setup
  writes `~/.claude/settings.json` only (records from other `~/.claude*`
  folders are listed).
