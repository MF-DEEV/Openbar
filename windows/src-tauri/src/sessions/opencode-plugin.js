// Vorssaint Dynamic Island: shows OpenCode's sessions in the island's Sessions list.
// Installed and removed from the island's settings. __VORSSAINT_MARKER__
import { connect } from "node:net";

const SOCKET = "__VORSSAINT_SOCKET__";
const LIMIT = 4000;
// A file change's patch, shown as a diff on the permission card.
const PATCH_LIMIT = 200000;

export const VorssaintIsland = async ({ client, directory }) => {
  const pid = process.pid;
  const children = new Set();
  const roles = new Map();
  const sent = new Map();
  const waiting = new Map();
  // What the island was told of each session, told again after it restarts.
  const known = new Map();
  const clip = (text) => (typeof text === "string" ? text.slice(0, LIMIT) : undefined);
  const clipPatch = (text) => (typeof text === "string" ? text.slice(0, PATCH_LIMIT) : undefined);
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  // One event on its own connection. One that waits gets the island's answer back.
  const send = (event, wait, key) =>
    new Promise((resolve) => {
      let reply = "";
      let done = false;
      const finish = (value) => {
        if (done) return;
        done = true;
        if (key) waiting.delete(key);
        resolve(value);
      };
      let socket;
      try {
        socket = connect(SOCKET);
      } catch {
        return finish(null);
      }
      if (key) waiting.set(key, socket);
      socket.on("error", () => finish(null));
      socket.on("connect", () => {
        socket.end(JSON.stringify({ vorssaint: "opencode", pid, event: { directory, at: Date.now(), ...event } }));
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

  // Sends what changed only: OpenCode repeats a part with every streamed word.
  const post = (event, dedupe) => {
    if (dedupe) {
      const value = JSON.stringify(event);
      if (sent.get(dedupe) === value) return;
      sent.set(dedupe, value);
      if (sent.size > 2000) sent.clear();
    }
    void send(event, false);
  };

  const announce = () => {
    for (const [session, state] of known) {
      void send({ kind: "created", session, directory: state.directory }, false);
      if (state.status) void send({ kind: "status", session, status: state.status }, false);
      if (state.title) void send({ kind: "title", session, title: state.title }, false);
    }
  };

  // Replies the person types in the island, for this OpenCode's sessions.
  // Each time the island is reached anew, as after it restarts, it hears of them again.
  const listen = async () => {
    let fresh = true;
    for (;;) {
      if (fresh) announce();
      const answer = await send({ kind: "listen" }, true);
      fresh = !answer;
      const prompt = answer && answer.prompt;
      if (prompt && prompt.session && prompt.text) {
        try {
          await client.session.promptAsync({
            path: { id: prompt.session },
            body: { parts: [{ type: "text", text: prompt.text }] },
          });
        } catch {}
        continue;
      }
      await sleep(answer ? 1000 : 5000);
    }
  };
  void listen();

  const ask = async (p) => {
    const answer = await send(
      {
        kind: "permission",
        session: p.sessionID,
        id: p.id,
        permission: p.permission ?? p.type,
        patterns: Array.isArray(p.patterns) ? p.patterns.map(clip) : p.pattern ? [].concat(p.pattern).map(clip) : [],
        metadata: {
          command: clip(p.metadata && p.metadata.command),
          filepath: clip(p.metadata && (p.metadata.filepath ?? p.metadata.filePath)),
          diff: clipPatch(p.metadata && p.metadata.diff),
          files:
            p.metadata && Array.isArray(p.metadata.files)
              ? p.metadata.files.slice(0, 50).map((f) => ({
                  filePath: clip(f && (f.filePath ?? f.relativePath)),
                  type: clip(f && f.type),
                  patch: clipPatch(f && f.patch),
                  movePath: clip(f && f.movePath),
                }))
              : undefined,
        },
        always: Array.isArray(p.always) ? p.always.map(clip) : [],
      },
      true,
      p.id,
    );
    if (!answer || !answer.reply) return;
    try {
      await client.postSessionIdPermissionsPermissionId({
        path: { id: p.sessionID, permissionID: p.id },
        body: { response: answer.reply },
      });
    } catch {}
  };

  return {
    event: async ({ event }) => {
      const p = event.properties ?? {};
      const info = p.info ?? {};
      const session = p.sessionID ?? info.sessionID ?? (event.type.startsWith("session.") ? info.id : undefined);
      if (event.type === "session.created" && info.parentID) children.add(info.id);
      if (!session || children.has(session)) return;
      const state = known.get(session) ?? { directory: info.directory ?? directory };
      if (event.type === "session.deleted") known.delete(session);
      else {
        if (event.type === "session.status") state.status = p.status && p.status.type;
        if (event.type === "session.idle") state.status = "idle";
        if (event.type === "session.updated" && info.title && !/^New session - /.test(info.title))
          state.title = clip(info.title);
        known.set(session, state);
        if (known.size > 200) known.delete(known.keys().next().value);
      }
      switch (event.type) {
        case "session.created":
          return post({ kind: "created", session, directory: info.directory ?? directory });
        case "session.deleted":
          return post({ kind: "deleted", session });
        case "session.status":
          return post({ kind: "status", session, status: p.status && p.status.type }, "status:" + session);
        case "session.idle":
          return post({ kind: "status", session, status: "idle" }, "status:" + session);
        case "session.error":
          return post({ kind: "error", session });
        case "session.updated":
          if (info.title && !/^New session - /.test(info.title))
            post({ kind: "title", session, title: clip(info.title) }, "title:" + session);
          return;
        case "message.updated":
          roles.set(info.id, info.role);
          if (roles.size > 2000) roles.clear();
          if (info.role === "assistant")
            post(
              { kind: "message", session, id: info.id, model: info.modelID, cost: info.cost, tokens: info.tokens },
              "message:" + info.id,
            );
          return;
        case "message.part.updated": {
          const part = p.part ?? {};
          if (part.type === "tool" && part.state && part.state.status !== "pending") {
            return post(
              {
                kind: "tool",
                session,
                call: part.callID,
                tool: part.tool,
                status: part.state.status,
                input: part.state.status === "running" ? summarize(part.state.input) : undefined,
              },
              "tool:" + part.callID + ":" + part.state.status,
            );
          }
          if (part.type === "text" && !part.synthetic) {
            const role = roles.get(part.messageID);
            if (role === "user") return post({ kind: "prompt", session, text: clip(part.text) }, "text:" + part.id);
            if (role === "assistant" && part.time && part.time.end)
              return post({ kind: "reply", session, text: clip(part.text) }, "text:" + part.id);
          }
          return;
        }
        case "permission.asked":
        case "permission.updated":
          if (p.id && !waiting.has(p.id)) void ask(p);
          return;
        case "permission.replied": {
          const id = p.requestID ?? p.permissionID ?? p.id;
          const socket = waiting.get(id);
          if (socket) socket.destroy();
          return post({ kind: "permissionReplied", session, id });
        }
        case "question.asked":
          return post({ kind: "question", session });
        case "question.replied":
        case "question.rejected":
          return post({ kind: "permissionReplied", session });
      }
    },
  };
};

// What a tool works on, without the whole of what it was given.
function summarize(input) {
  if (!input || typeof input !== "object") return undefined;
  const out = {};
  for (const key of ["command", "filePath", "path", "pattern", "url", "query", "description"]) {
    if (typeof input[key] === "string") out[key] = input[key].slice(0, 400);
  }
  return out;
}
