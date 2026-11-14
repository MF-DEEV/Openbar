// Openbar Dynamic Island: shows OpenCode's sessions in the island's Sessions list.
// Installed and removed from the island's settings. __OPENBAR_MARKER__
import { connect } from "node:net";
import { createHash } from "node:crypto";

const SOCKET = "__OPENBAR_SOCKET__";
const LIMIT = 4000;
// A file change's patch, shown as a diff on the permission card.
const PATCH_LIMIT = 200000;

export const OpenbarIsland = async ({ client, directory }) => {
  const pid = process.pid;
  const children = new Set();
  const roles = new Map();
  const sent = new Map();
  const waiting = new Map();
  const permissionVersions = new Map();
  // What the island was told of each session, told again after it restarts.
  const known = new Map();
  const clip = (text) => (typeof text === "string" ? text.slice(0, LIMIT) : undefined);
  const clipPatch = (text) => (typeof text === "string" ? text.slice(0, PATCH_LIMIT) : undefined);
  const scalarCount = (text) => {
    let count = 0;
    for (const _ of text) count++;
    return count;
  };
  const scalarPrefix = (text, limit = 8000) => {
    let out = "",
      count = 0;
    for (const c of text) {
      if (count++ === limit) break;
      out += c;
    }
    return out;
  };
  const patternPreview = (values) => {
    let out = "",
      count = 0;
    outer: for (let i = 0; i < values.length; i++) {
      if (i) {
        if (count++ === 8000) break;
        out += "\n";
      }
      if (typeof values[i] !== "string") continue;
      for (const c of values[i]) {
        if (count++ === 8000) break outer;
        out += c;
      }
    }
    return out;
  };
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  // One event on its own connection. One that waits gets the island's answer back.
  const send = (event, wait, key) =>
    new Promise((resolve) => {
      let reply = "";
      let done = false;
      const finish = (value) => {
        if (done) return;
        done = true;
        if (key && waiting.get(key) === socket) waiting.delete(key);
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
        const data = JSON.stringify({ openbar: "opencode", pid, event: { directory, at: Date.now(), ...event } });
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

  const ask = async (p, version) => {
    const budget = 1048576;
    const validMetadata = p.metadata == null || (typeof p.metadata === "object" && !Array.isArray(p.metadata));
    const metadata = validMetadata ? (p.metadata ?? {}) : {};
    const command = metadata.command;
    const commandComplete =
      typeof command === "string" &&
      command.length > 0 &&
      Buffer.byteLength(command, "utf8") <= budget &&
      Buffer.byteLength(JSON.stringify(command), "utf8") <= 3 * budget;
    const originalPatterns = Array.isArray(p.patterns) ? p.patterns : p.pattern != null ? [].concat(p.pattern) : [];
    const validPatterns =
      (p.patterns == null || Array.isArray(p.patterns)) && originalPatterns.every((value) => typeof value === "string");
    const patternBytes = originalPatterns.reduce(
      (n, value) => n + (typeof value === "string" ? Buffer.byteLength(value, "utf8") + 1 : 0),
      0,
    );
    const patternsComplete =
      validPatterns && patternBytes <= budget && Buffer.byteLength(JSON.stringify(originalPatterns), "utf8") <= budget;
    // Fallback metadata must be kept whole, including fields unknown to this plugin.
    // Otherwise it cannot be declared complete merely because its projection is short.
    let fullMetadata = null;
    if (!Object.hasOwn(metadata, "command") && !originalPatterns.length && Object.keys(metadata).length) {
      try {
        if (
          Buffer.byteLength(JSON.stringify(metadata), "utf8") <= budget &&
          !Object.hasOwn(metadata, "commandComplete") &&
          !Object.hasOwn(metadata, "commandLength")
        )
          fullMetadata = metadata;
      } catch {
        /* Malformed data is explicitly incomplete below. */
      }
    }
    const permissionContentComplete =
      validMetadata &&
      validPatterns &&
      (Object.hasOwn(metadata, "command")
        ? commandComplete
        : originalPatterns.length
          ? patternsComplete
          : fullMetadata !== null);
    const answer = await send(
      {
        kind: "permission",
        session: p.sessionID,
        id: p.id,
        permission: p.permission ?? p.type,
        permissionContentComplete,
        permissionDetailLength:
          originalPatterns.length && validPatterns
            ? originalPatterns.reduce((n, value) => n + scalarCount(value), originalPatterns.length - 1)
            : undefined,
        patterns: patternsComplete ? originalPatterns : [patternPreview(originalPatterns)],
        metadata: {
          ...(fullMetadata ?? {
            filepath: clip(metadata.filepath ?? metadata.filePath),
            diff: clipPatch(metadata.diff),
            files: Array.isArray(metadata.files)
              ? metadata.files.slice(0, 50).map((f) => ({
                  filePath: clip(f && (f.filePath ?? f.relativePath)),
                  type: clip(f && f.type),
                  patch: clipPatch(f && f.patch),
                  movePath: clip(f && f.movePath),
                }))
              : undefined,
          }),
          // Permission commands bypass the summary clip. Oversized data is explicitly incomplete.
          command: typeof command === "string" ? (commandComplete ? command : scalarPrefix(command)) : undefined,
          commandComplete,
          commandLength: typeof command === "string" ? scalarCount(command) : undefined,
        },
        always: Array.isArray(p.always) ? p.always.map(clip) : [],
      },
      true,
      p.id,
    );
    if (permissionVersions.get(p.id)?.version !== version || !answer || !answer.reply) return;
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
          for (const [id, pending] of permissionVersions) {
            if (pending.session !== session) continue;
            permissionVersions.delete(id);
            waiting.get(id)?.destroy();
          }
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
        case "permission.updated": {
          if (!p.id) return;
          let fingerprint;
          try {
            fingerprint = createHash("sha256")
              .update(
                JSON.stringify({
                  session,
                  permission: p.permission ?? p.type,
                  metadata: p.metadata,
                  patterns: p.patterns,
                  pattern: p.pattern,
                  always: p.always,
                }),
              )
              .digest("hex");
          } catch {
            permissionVersions.delete(p.id);
            waiting.get(p.id)?.destroy();
            return;
          }
          if (permissionVersions.get(p.id)?.fingerprint === fingerprint && waiting.has(p.id)) return;
          const version = Symbol();
          permissionVersions.set(p.id, { fingerprint, version, session });
          waiting.get(p.id)?.destroy();
          void ask(p, version);
          return;
        }
        case "permission.replied": {
          const id = p.requestID ?? p.permissionID ?? p.id;
          permissionVersions.delete(id);
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
