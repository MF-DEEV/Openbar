# Sessions in containers

Issue #330. Claude Code and Codex sessions running inside Docker containers
and devcontainers show up in the island next to local ones, with the
container's name on them. Their permission requests can be allowed or denied
from the island, and replies reach the ones running inside tmux. It works
like [remote hosts](remote-hosts.md), with `docker exec` in place of `ssh`,
on the Mac, Windows and Linux.

## Setting it up

1. Settings → Dynamic Island → Content → Sessions → **Show sessions in
   containers** (on Windows and Linux: Settings → Sessions → **Containers**).
   It is off by default.
2. Under **Running containers**, press **Add** next to a container. A
   devcontainer is listed by its project and goes by its folder on this
   machine, so it stays picked when it is rebuilt with a new name and ID. Any
   other container goes by its name. Openbar runs nothing in a container you
   didn't add.
3. Once it shows as connected, press **Set up** and confirm. This adds
   Openbar's hooks to `~/.claude/settings.json` of the container's user and
   keeps a copy of the file as it was, inside the container. Nothing else
   changes there, apart from the `~/.openbar` folder the relay uses.
4. **Remove from container** takes the hooks out. If nothing else in the file
   changed, the copy goes back byte for byte, keeping the file's mode (or the
   file is deleted if there was none). Otherwise only Openbar's entries come
   out.

Openbar uses the `docker` command line it finds, or `podman`, which takes the
same commands. Docker Desktop, OrbStack, Colima and Rancher Desktop all
install a `docker`, and the command line follows its own current context.
A Mac app doesn't inherit your shell's `PATH`, so Openbar looks in the usual
places: `/usr/local/bin`, `/opt/homebrew/bin`, `~/.orbstack/bin`,
`~/.docker/bin`, Docker.app and `~/.rd/bin`, then Podman's. On Windows it
looks in Docker Desktop's and Rancher Desktop's folders, Podman's, and `PATH`;
on Linux in `/usr/bin`, `/usr/local/bin` and `PATH`.

## How it works

For each container Openbar picks, it lists the running containers
(`docker ps -q` and `docker inspect`) and finds the one that matches, by the
`devcontainer.local_folder` label or by name. Then it starts three kinds of
`docker exec -i`, all running the same loader as an SSH host:

```
docker exec -i [-u <remoteUser>] -- <container> sh -c 'IFS= read -r s; eval "$(printf %s "$s" | base64 -d)"'
```

Openbar sends [`Resources/remote-agent.sh`](../Resources/remote-agent.sh) as
one base64 line, then a mode:

- `probe`, at each (re)connect: the home folder, the system, tmux, Codex's
  socket, and whether the hooks are set up. The probe of a container looks
  for its own marker (below).
- `agent`, which stays: the same snapshot, paste, enter and focus requests
  as on an SSH host. Sessions, transcripts, the model, tokens and cost come
  through it every 1.5 seconds.
- `relay`, which stays too and carries the hooks.

In a devcontainer Openbar runs as the user VS Code runs as: the last
`remoteUser` in the `devcontainer.metadata` label. Any other container runs as
its own user.

### The relay

`docker exec` can't forward a socket the way `ssh -R` does, and Docker Desktop
runs containers in a VM, so a host socket can't be mounted into one either.
Instead, the relay mode (plain POSIX `sh`) uses no socket at all. It makes a FIFO in
`~/.openbar` (mode 700) and writes a small hook script next to it,
`~/.openbar/<app>.hook`. Each hook Claude Code runs is:

```
sh '/home/node/.openbar/<app>.hook' 2>/dev/null; exit 0 # openbar-container-hook <app>
```

The hook writes its event to `~/.openbar/<app>.q/<pid>.req` and its process
number to the FIFO, which the relay is blocked reading, so nothing polls until
an answer is due. The relay sends the event to the island as one line,
`H <pid> <base64>`. The island hands it to its hook server as if the hook had
connected: on the Mac as one end of a socket pair (`ClaudeHookServer.adopt`),
on Windows and Linux as a `HookConnection` whose answer goes back when the
island is done with it. So every rule about which hooks wait and how they're
answered stays the same as for a local hook. The answer comes back as
`A <pid> <base64>` (or `-` for none), the relay writes it to
`<pid>.res`, and the hook prints it.

The hook needs only `sh`, `base64`, `mv` and `mkfifo`, which every image with a
shell has, Alpine's busybox included. It checks that the relay's process is
alive before it writes anything and while it waits, so when no island is
connected it ends at once and Claude Code asks in its terminal. When the
island goes away, the relay's standard input closes; it removes its FIFO and
process file and ends, and any hook still waiting lets go.

The container's hooks carry the marker `# openbar-container-hook <app>`, not
the SSH hosts' `# openbar-remote-hook <app>`. A machine reached both ways never
takes one kind for the other, and while either kind is left in the file, the
copy kept at the first setup stays for the last removal.

### Codex in a container

Codex's terminal UIs share an app-server daemon (see
[codex-sessions.md](codex-sessions.md)). Each look at the container says
whether its socket is there. While it is, Openbar joins it through one more
`docker exec -i` running the script's `codex` mode, which bridges standard
input and output to the socket with `socat`, `nc -U`, `python3` or `node`
(Codex installed with npm brings node). On the Mac the island's end is a
socket pair, so the Codex client talks to it as to any socket. Threads, the
approvals with their diff, questions, replies and Always work as they do
locally, and the threads carry the container's name.

### Containers come and go

When a container stops, its link ends, its sessions leave the list and
Settings says why. Openbar looks for it again after 2, 4, 8, 16 and then every
30 seconds. A rebuilt devcontainer is found again by its folder. Unless its
home folder is a volume, the rebuild took the hooks with it: Settings then
says the container was rebuilt and lost Openbar's hooks, and offers
**Set up again**. Openbar remembers the ID of the container it set up, which
is how it tells a rebuilt container from hooks taken out by hand.

To keep the hooks across rebuilds, keep the home folder on a volume, for
example in `devcontainer.json`:

```json
"mounts": ["source=claude-home-${devcontainerId},target=/home/node/.claude,type=volume"]
```

Claude Code's own reference devcontainer already keeps `~/.claude` on a volume
this way, so its hooks survive a rebuild.

## Replies and Open

- **Inside tmux in the container:** pasted with `tmux load-buffer` and
  `paste-buffer`, then Enter, through the agent, as on an SSH host.
- **Anywhere else:** the reply field says replies into a container reach
  sessions inside tmux only. Permissions still work.
- **Open** brings the devcontainer's VS Code window forward
  (`vscode://vscode-remote/dev-container+<folder in hex>/<workspace>`), when
  something on this machine opens `vscode://` links. Otherwise, for a session
  in tmux, it opens a terminal attached to it:
  `docker exec -it [-u <user>] <container> tmux -u -S <socket> attach-session -t <pane>`.
  On the Mac the island first offers **Attach**, as for a detached tmux.

VS Code's integrated terminal in a devcontainer isn't reached for replies.
The terminal bridge extension is a UI extension, so it sees the window's
terminals, but it tells them apart by the shell's process number, and a
container's numbers can match this machine's. It would need to know which
container a window is attached to first.

## What was checked, and how

The containers ran the real Claude Code 2.1.332 and Codex 0.172 (installed
with npm in a `node:22-bookworm-slim` image) inside tmux, pointed at a local
stand-in for the Messages and Responses APIs that asks to run a command. No
account and no tokens were used.

On the Mac, with Docker under Colima, using a copy of the app with its own
bundle ID:

- A plain container and a devcontainer made by the official devcontainers CLI
  (the one VS Code uses) were listed, added and connected in about 0.15 s.
- Set up asked first, added 11 hooks and kept the copy. Remove left
  `~/.claude/settings.json` identical (same SHA-256) and with its mode.
- Allow in the island: Claude Code said `Allowed by PermissionRequest hook`
  and ran the command. Deny: `Denied by PermissionRequest hook`, nothing ran,
  and the turn went on.
- A reply typed in the island arrived in the tmux pane and Claude answered it.
- Codex in the container showed up with the container's name, and Allow ran
  its command.
- Rebuilding the devcontainer (`devcontainer up --remove-existing-container`):
  the link found the new container within two seconds, Settings said it was
  rebuilt and lost the hooks, and Set up again put them back.
- Open attached a Terminal window to the session's tmux in the container.

On Linux (Ubuntu 24.04 arm64 in a VM, X11), with rootless Podman 4.9 and a
debug build of the app:

- A plain container and a devcontainer made by the devcontainers CLI with
  `--docker-path podman` showed up, grouped under their projects.
- Set up, Allow, Deny, a reply into tmux, Codex's approval, and Remove (same
  SHA-256, mode kept) worked as on the Mac.
- Removing the container and starting a new one with the same name: Settings
  said it was rebuilt and offered Set up again.

On Windows 11 ARM64 (in a VM), the Docker CLI for Windows was pointed at the
Mac's Colima over a loopback port, as a stand-in for Docker Desktop, which
can't run nested in that VM:

- Both containers showed up, the devcontainer under its project.
- Set up, Allow, Deny, a reply into tmux, Codex's approval, and Remove (same
  SHA-256, mode kept) worked.

The Mac, Rust and TypeScript unit tests cover the target names, the
`docker inspect` shapes of Docker, the devcontainers CLI and Podman, the
commands, both markers in one file, and the relay script itself under `sh`
with a hook going through it.

## Not done (follow-ups)

- Replies into VS Code's integrated terminal in a devcontainer (above).
- Sessions in WSL itself on Windows (see [terminal-hosts.md](terminal-hosts.md)).
- Images can't be attached to a container's sessions: their paths are on this
  machine.
- Containers whose Claude Code keeps its settings under `CLAUDE_CONFIG_DIR`:
  setup writes `~/.claude/settings.json` only.
