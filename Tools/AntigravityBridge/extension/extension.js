// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Each window's extension host listens on its own socket, readable by its
// owner alone. The island asks every socket for the terminal whose shell is
// the parent of a Claude Code process, and that window shows it or types
// into it. Nothing is read from the terminals and nothing leaves the Mac.
const vscode = require('vscode');
const net = require('net');
const fs = require('fs');
const os = require('os');
const path = require('path');

const ESC = '\u001b';
const MAX_REQUEST = 64 * 1024;
let server;
let socketPath;

function activate(context) {
  const folder = path.join(os.homedir(), '.vorssaint', 'terminals');
  fs.mkdirSync(folder, { recursive: true, mode: 0o700 });
  fs.chmodSync(folder, 0o700);
  socketPath = path.join(folder, `${process.pid}.sock`);
  try { fs.unlinkSync(socketPath); } catch (_) {}
  server = net.createServer((connection) => {
    let buffer = '';
    connection.setEncoding('utf8');
    connection.setTimeout(5000, () => connection.destroy());
    connection.on('data', (chunk) => {
      buffer += chunk;
      if (buffer.length > MAX_REQUEST) return connection.destroy();
      const end = buffer.indexOf('\n');
      if (end < 0) return;
      const line = buffer.slice(0, end);
      buffer = '';
      handle(line)
        .then((reply) => connection.end(JSON.stringify(reply) + '\n'))
        .catch((error) => connection.end(JSON.stringify({ ok: false, error: String(error) }) + '\n'));
    });
    connection.on('error', () => {});
  });
  const previous = process.umask(0o077);
  server.listen(socketPath, () => {
    process.umask(previous);
    try { fs.chmodSync(socketPath, 0o600); } catch (_) {}
  });
  server.on('error', () => {});
  context.subscriptions.push({ dispose: deactivate });
}

async function terminalFor(shellPid) {
  for (const terminal of vscode.window.terminals) {
    if ((await terminal.processId) === shellPid) return terminal;
  }
  return undefined;
}

async function handle(line) {
  const request = JSON.parse(line);
  if (request.command === 'ping') return { ok: true };
  const terminal = await terminalFor(request.shellPid);
  if (!terminal) return { ok: false, error: 'missing' };
  if (request.command === 'focus') {
    terminal.show(false);
    return { ok: true };
  }
  if (request.command === 'send' && typeof request.text === 'string' && request.text.length > 0) {
    // A paste keeps line breaks as text; the return after it sends the reply.
    terminal.sendText(`${ESC}[200~${request.text}${ESC}[201~`, false);
    await new Promise((resolve) => setTimeout(resolve, 80));
    terminal.sendText('\r', false);
    return { ok: true };
  }
  return { ok: false, error: 'unknown' };
}

function deactivate() {
  if (server) server.close();
  server = undefined;
  if (socketPath) { try { fs.unlinkSync(socketPath); } catch (_) {} }
}

module.exports = { activate, deactivate };
