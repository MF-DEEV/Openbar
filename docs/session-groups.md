# Session groups and branches

The Sessions list can stay one column or group its rows by project or by
agent (#233). The three marks at the left of the list's band switch between
them, and Settings → Sessions has the same choice. One list is the default.

## Groups

- By project puts sessions with the same project name together. A
  worktree made by Claude Code, the island or Gemini CLI counts as its
  repository, so every session of one repo lands in one group. A project on
  a remote host (#189) is its own group, with the host's badge in the header.
- By agent puts each agent's sessions together under its mark and name.
- Groups follow the order of their first session in the usual list, so a
  group with someone waiting is on top. Inside a group the order doesn't
  change.
- A header shows the group's mark, name and session count. If a session is
  waiting for the person, the header also shows an amber hand with how many.
- Pressing a header folds the group. A folded group shows its sessions as
  small faces ringed in their phase's colour, and keeps the amber outline
  while one of them waits. Folded groups are remembered by key
  (`project:<host>:<name>` or `agent:<agent>`) across launches: on the Mac in
  `notchSessionsCollapsedGroups`, on Windows and Linux in `collapsedGroups`
  in settings.json. Only the latest 64 folds are kept.
- In a project's group the rows leave out the project tag, since the header
  already names it.

## The branch

Every row shows the branch next to the project, and so does the session's
page, enlarged or not. A long name is cut in the middle so its start and end
stay readable, and the whole name shows on hover and is read out by
VoiceOver and screen readers. In a worktree the row shows the worktree's
name too, unless the branch already carries it (`worktree-<name>`,
`openbar/<name>`).

For sessions on this machine the island reads the repository's `HEAD` every
few seconds (a linked worktree's own `HEAD` included), so a checkout inside a
session shows on its row:

| `HEAD` | Row |
| --- | --- |
| a branch | that branch |
| a commit (detached) | nothing, or only the worktree's name |
| no repository, or unreadable | what the agent reported, if anything |

Sessions on a remote host keep the branch their host reports: Claude Code's
`gitBranch` and Codex's `gitInfo`.

Code: `NotchSessionGroups.swift`, `NotchSessionGroupViews.swift` and
`AgentGitBranch` on the Mac; `sessions/groups.ts`, `SessionGroups.tsx` and
`hook_agents::read_head` on Windows and Linux.
