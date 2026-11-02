# Security policy

Openbar runs on your machine, talks to coding agents over local sockets and can answer their permission prompts. A bug that lets something else answer those prompts, read session content, or misuse the app's macOS or Windows permissions is a security bug. Please treat it as one.

## Reporting a vulnerability

Report it privately. Don't open a public issue, pull request or discussion.

**[Open a private security advisory](https://github.com/MF-DEEV/Openbar/security/advisories/new)**

Only you and the maintainer can see it. Please include:

- what the issue is and what an attacker could do with it,
- steps to reproduce, or a proof of concept,
- the commit or Windows release you tested, your OS version, and the agent and its version if Sessions is involved.

## What to expect

This is a community project looked after on a best-effort basis. You'll get a reply as soon as possible, and a fix lands on `main` before the issue is discussed in public. Reporters are credited if they want to be.

## Supported versions

Fixes land on `main` and in the next Windows release. On the Mac, Openbar is built from source, so please check that the issue still happens on the latest `main`.

## Scope

In scope:

- the local hook socket, Codex app-server and OpenCode plugin bridges, and anything that decides which agent request gets which answer;
- reading or keeping session content beyond what [the privacy notes](docs/PRIVACY.md) describe;
- the Windows installer and its elevated helper tasks.

Out of scope: bugs in the agents themselves (Claude Code, Codex and the rest) and in outside services the app only talks to. Please report those to their makers.
