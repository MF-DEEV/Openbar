// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The extension the island puts into Pi's extensions folder. It runs inside
/// Pi, tells the island's socket what the session does, in the same few
/// fields OpenCode's plugin writes, and waits on the socket for a reply the
/// person types in the island, which it hands to Pi with
/// `pi.sendUserMessage`. Pi asks no permission of its own; a dialog another
/// extension opens is told as a wait, answered in Pi. When the island is not
/// running it connects to nothing and tries again a few seconds later, and
/// it never keeps Pi from quitting. The same file as
/// `windows/src-tauri/src/sessions/pi-extension.js`; a Rust test keeps them alike.
enum AgentPiExtension {
    static let source = #"""
// Openbar Dynamic Island: shows Pi's sessions in the island's Sessions list.
// Installed and removed from the island's settings. __OPENBAR_MARKER__
import { connect } from "node:net";

const SOCKET = "__OPENBAR_SOCKET__";
const LIMIT = 4000;

export default function openbarIsland(pi) {
  const pid = process.pid;
  // The session this Pi runs now, and the context its events last came with.
  let current;
  let context;
  let lastStop;
  let messages = 0;
  const clip = (text) => (typeof text === "string" ? text.slice(0, LIMIT) : undefined);
  const sleep = (ms) =>
    new Promise((resolve) => {
      const timer = setTimeout(resolve, ms);
      if (timer.unref) timer.unref();
    });

  // One event on its own connection. One that waits gets the island's answer back.
  const send = (event, wait) =>
    new Promise((resolve) => {
      let reply = "";
      let done = false;
      const finish = (value) => {
        if (done) return;
        done = true;
        resolve(value);
      };
      let socket;
      try {
        socket = connect(SOCKET);
      } catch {
        return finish(null);
      }
      // Pi quits when its work is done, whether or not the island answered.
      socket.unref();
      socket.on("error", () => finish(null));
      socket.on("connect", () => {
        const data = JSON.stringify({ openbar: "pi", pid, event: { at: Date.now(), ...event } });
        // A Windows pipe closes as a whole on end(), so one that waits for an answer only writes.
        if (wait) socket.write(data);
        else socket.end(data);
      });
      socket.on("data", (chunk) => {
        reply += chunk;
      });
      socket.on("close", () => {
        try {
          finish(reply ? JSON.parse(reply) : null);
        } catch {
          finish(null);
        }
      });
      if (!wait) socket.setTimeout(3000, () => socket.destroy());
    });

  const post = (event) => {
    if (!current) return;
    void send({ session: current.session, directory: current.directory, ...event }, false);
  };

  const begin = (ctx) => {
    context = ctx;
    const manager = ctx.sessionManager;
    const session = manager && manager.getSessionId && manager.getSessionId();
    if (!session) return;
    current = { session, directory: ctx.cwd, title: clip(manager.getSessionName && manager.getSessionName()) };
    announce();
  };

  // What the island needs to list this session, told again whenever it is reached anew.
  const announce = () => {
    if (!current) return;
    post({ kind: "created" });
    const model = context && context.model;
    if (model && model.id) post({ kind: "model", model: model.id });
    if (current.title) post({ kind: "title", title: current.title });
    if (context && !context.isIdle()) post({ kind: "status", status: "busy" });
  };

  // Replies the person types in the island, for this Pi's session.
  const listen = async () => {
    let fresh = false;
    for (;;) {
      if (fresh) announce();
      const answer = await send({ kind: "listen" }, true);
      fresh = !answer;
      const prompt = answer && answer.prompt;
      if (prompt && current && prompt.session === current.session && prompt.text) {
        try {
          const idle = !context || context.isIdle();
          pi.sendUserMessage(prompt.text, idle ? undefined : { deliverAs: "followUp" });
        } catch {}
        continue;
      }
      await sleep(answer ? 1000 : 5000);
    }
  };
  void listen();

  pi.on("session_start", (_event, ctx) => {
    begin(ctx);
  });
  pi.on("session_shutdown", () => {
    post({ kind: "deleted" });
    current = undefined;
  });
  pi.on("session_info_changed", (event, ctx) => {
    context = ctx;
    if (!current) return;
    current.title = clip(event.name);
    if (current.title) post({ kind: "title", title: current.title });
  });
  pi.on("model_select", (event, ctx) => {
    context = ctx;
    if (event.model && event.model.id) post({ kind: "model", model: event.model.id });
  });
  pi.on("before_agent_start", (event, ctx) => {
    context = ctx;
    if (event.messageType === "user") post({ kind: "prompt", text: clip(event.prompt) });
  });
  pi.on("agent_start", (_event, ctx) => {
    context = ctx;
    lastStop = undefined;
    post({ kind: "status", status: "busy" });
  });
  pi.on("agent_settled", (_event, ctx) => {
    context = ctx;
    post(lastStop === "error" ? { kind: "error" } : { kind: "status", status: "idle" });
  });
  pi.on("tool_execution_start", (event, ctx) => {
    context = ctx;
    if (event.parentToolCallId) return;
    post({ kind: "tool", call: event.toolCallId, tool: event.toolName, status: "running", input: summarize(event.args) });
  });
  pi.on("tool_execution_end", (event, ctx) => {
    context = ctx;
    if (event.parentToolCallId) return;
    post({ kind: "tool", call: event.toolCallId, tool: event.toolName, status: event.isError ? "error" : "completed" });
  });
  pi.on("message_end", (event, ctx) => {
    context = ctx;
    const message = event.message;
    if (!message || message.role !== "assistant") return;
    lastStop = message.stopReason;
    const usage = message.usage;
    messages += 1;
    post({
      kind: "message",
      id: `${current ? current.session : ""}:${messages}:${message.timestamp || Date.now()}`,
      model: message.model,
      cost: usage && usage.cost ? usage.cost.total : 0,
      tokens: usage
        ? {
            input: usage.input,
            // Pi counts thinking inside its output; the island adds the two.
            output: (usage.output || 0) - (usage.reasoning || 0),
            reasoning: usage.reasoning || 0,
            cache: { read: usage.cacheRead, write: usage.cacheWrite },
          }
        : undefined,
    });
    const text = Array.isArray(message.content)
      ? message.content
          .filter((part) => part && part.type === "text" && typeof part.text === "string")
          .map((part) => part.text)
          .join("\n")
          .trim()
      : "";
    if (text) post({ kind: "reply", text: clip(text) });
  });
  // An extension's dialog waits on the person, in Pi.
  pi.on("ui_prompt_start", (_event, ctx) => {
    context = ctx;
    post({ kind: "question" });
  });
  pi.on("ui_prompt_end", (_event, ctx) => {
    context = ctx;
    post({ kind: "permissionReplied" });
  });
}

// What a tool works on, without the whole of what it was given.
function summarize(input) {
  if (!input || typeof input !== "object") return undefined;
  const out = {};
  for (const key of ["command", "path", "filePath", "file_path", "pattern", "url", "query", "description"]) {
    if (typeof input[key] === "string") out[key] = input[key].slice(0, 400);
  }
  return out;
}
"""#
}
