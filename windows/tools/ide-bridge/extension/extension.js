// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Windows twin of Tools/AntigravityBridge. Each window's extension host
// listens on a named pipe of its own and writes where it listens, with a
// random token, to a file in the user's own app data. The island reads the
// file, asks each window for the terminal whose shell is one of a Claude
// Code process's ancestors, and that window shows it or types into it. A
// request without the token is refused; nothing is read from the terminals
// and nothing leaves the PC.
const vscode = require('vscode');
const net = require('net');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');

const ESC = '\u001b';
const MAX_REQUEST = 64 * 1024;
const VERSION = require('./package.json').version;
let server;
let registration;

function registryFolder() {
  const base = process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local');
  return path.join(base, 'Vorssaint', 'terminals');
}

function ideName() {
  const app = (vscode.env.appName || '').toLowerCase();
  if (app.includes('cursor')) return 'cursor';
  if (app.includes('antigravity')) return 'antigravity';
  if (app.includes('windsurf')) return 'windsurf';
  return 'vscode';
}

function activate(context) {
  const user = (process.env.USERNAME || 'user').replace(/[^A-Za-z0-9]/g, '');
  const pipe = `\\\\.\\pipe\\vorssaint-terminals-${user}-${process.pid}`;
  const token = crypto.randomBytes(24).toString('hex');
  server = net.createServer({ allowHalfOpen: true }, (connection) => {
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
      handle(line, token)
        .then((reply) => connection.end(JSON.stringify(reply) + '\n'))
        .catch((error) => connection.end(JSON.stringify({ ok: false, error: String(error) }) + '\n'));
    });
    connection.on('error', () => {});
  });
  server.on('error', () => {});
  server.listen(pipe, () => {
    const folder = registryFolder();
    fs.mkdirSync(folder, { recursive: true });
    registration = path.join(folder, `${process.pid}.json`);
    const body = { pipe, token, version: VERSION, ide: ideName(), pid: process.pid };
    fs.writeFileSync(registration, JSON.stringify(body), { encoding: 'utf8' });
  });
  context.subscriptions.push({ dispose: deactivate });
}

async function terminalFor(request) {
  const pids = new Set(Array.isArray(request.shellPids) ? request.shellPids : []);
  if (typeof request.shellPid === 'number') pids.add(request.shellPid);
  for (const terminal of vscode.window.terminals) {
    if (pids.has(await terminal.processId)) return terminal;
  }
  return undefined;
}

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function handle(line, token) {
  const request = JSON.parse(line);
  if (request.token !== token) return { ok: false, error: 'denied' };
  if (request.command === 'ping') return { ok: true, version: VERSION };
  const terminal = await terminalFor(request);
  if (!terminal) return { ok: false, error: 'missing' };
  if (request.command === 'focus') {
    terminal.show(false);
    return { ok: true };
  }
  if (request.command === 'steps' && Array.isArray(request.steps)) {
    for (const step of request.steps) {
      if (typeof step.paste === 'string') terminal.sendText(`${ESC}[200~${step.paste}${ESC}[201~`, false);
      else if (typeof step.text === 'string') terminal.sendText(step.text, false);
      else if (step.enter === true) terminal.sendText('\r', false);
      if (typeof step.wait === 'number' && step.wait > 0) await wait(Math.min(step.wait, 10000));
    }
    return { ok: true };
  }
  if (
    request.command === 'send-images' &&
    typeof request.text === 'string' &&
    Array.isArray(request.images) &&
    request.images.length > 0 &&
    request.images.every((image) => typeof image === 'string' && image.length > 0)
  ) {
    // Claude Code attaches an image when a paste is only its path, and reads
    // it a moment later, so the text and the return wait for the images.
    for (const image of request.images) {
      terminal.sendText(`${ESC}[200~${image}${ESC}[201~`, false);
      await wait(60);
    }
    await wait(1200 + 300 * request.images.length);
    if (request.text.length > 0) terminal.sendText(`${ESC}[200~${request.text}${ESC}[201~`, false);
    await wait(80);
    terminal.sendText('\r', false);
    return { ok: true };
  }
  if (request.command === 'send' && typeof request.text === 'string' && request.text.length > 0) {
    // A paste keeps line breaks as text; the return after it sends the reply.
    terminal.sendText(`${ESC}[200~${request.text}${ESC}[201~`, false);
    await wait(80);
    terminal.sendText('\r', false);
    return { ok: true };
  }
  return { ok: false, error: 'unknown' };
}

function deactivate() {
  if (server) server.close();
  server = undefined;
  if (registration) {
    try {
      fs.unlinkSync(registration);
    } catch (_) {}
    registration = undefined;
  }
}

module.exports = { activate, deactivate };
