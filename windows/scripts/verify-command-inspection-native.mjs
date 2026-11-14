// Run against the locally installed Tauri build, launched with
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9234.
// These permission requests terminate in this test process. No shell command executes.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { connect } from "node:net";
import { readdir, readFile, mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const windows = fileURLToPath(new URL("../", import.meta.url));
const output = resolve(windows, "../build/permission-security");
const installedSha256 = createHash("sha256")
  .update(await readFile(resolve(process.env.LOCALAPPDATA, "Openbar/Openbar.exe")))
  .digest("hex");
await mkdir(output, { recursive: true });
const endpoint = process.env.OPENBAR_CDP ?? "http://127.0.0.1:9234";
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(fn, label, timeout = 15000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const result = await fn();
    if (result) return result;
    await pause(100);
  }
  throw new Error(`Timed out: ${label}`);
}
const targets = await (await fetch(endpoint + "/json/list")).json();
const target = targets.find((t) => t.url.includes("/island.html"));
assert(target, "The actual Tauri island WebView must be running");
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((done, fail) => {
  ws.onopen = done;
  ws.onerror = fail;
});
let seq = 0;
const pending = new Map();
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  const call = pending.get(message.id);
  if (call) {
    pending.delete(message.id);
    message.error ? call.reject(message.error) : call.resolve(message.result);
  }
};
const cdp = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const r = await cdp("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const invoke = (name, args) =>
  evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)})`);
const sockets = [];
const id = `issue224-native-${process.pid}`;
const sessionId = `opencode:${id}`;
const pipe = `\\\\.\\pipe\\openbar-claude-hooks-com.openbar.island-${process.env.USERNAME.replace(/[^a-z0-9]/gi, "")}`;
function send(event) {
  const socket = connect(pipe);
  sockets.push(socket);
  let text = "";
  const reply = new Promise((done, fail) => {
    socket.on("data", (d) => (text += d));
    socket.on("error", fail);
    socket.on("close", () => done(text ? JSON.parse(text) : null));
  });
  socket.on("connect", () =>
    socket.write(
      JSON.stringify({
        openbar: "opencode",
        pid: process.pid,
        event: { at: Date.now(), directory: resolve(output, "harmless-fixture"), session: id, ...event },
      }),
    ),
  );
  return reply;
}
const checkSession = () => evaluate(`window.__issue224Snapshot?.sessions.find(s=>s.id===${JSON.stringify(sessionId)})`);
async function ask(request, command, complete = true, extra = {}) {
  const metadata = complete
    ? { command, commandComplete: true, commandLength: Array.from(command).length }
    : { command };
  const reply = send({
    kind: "permission",
    id: request,
    permission: "bash",
    metadata,
    always: ["harmless fixture"],
    ...extra,
  });
  await until(async () => (await checkSession())?.permission?.requestId === request, request);
  await evaluate(
    `window.__issue224Island.getState().open('sessions',{takeFocus:true});document.querySelector('.s-back')?.click();true`,
  );
  await until(
    () =>
      evaluate(
        "!!Array.from(document.querySelectorAll('.s-row')).find(r=>r.getAttribute('aria-label')?.includes('harmless-fixture'))",
      ),
    "fixture session row",
  );
  await evaluate(
    "Array.from(document.querySelectorAll('.s-row')).find(r=>r.getAttribute('aria-label')?.includes('harmless-fixture')).click();true",
  );
  await until(() => evaluate("!!document.querySelector('.s-card .s-button.is-allow')"), "permission card");
  await pause(1000);
  return { reply, p: (await checkSession()).permission };
}
async function click(selector) {
  assert(
    await evaluate(
      `(()=>{const b=document.querySelector(${JSON.stringify(selector)});if(!b||b.disabled)return false;const r=b.getBoundingClientRect();const hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(!r.width||!r.height||!hit||!b.contains(hit))return false;b.click();return true})()`,
    ),
    selector,
  );
  await pause(150);
}
async function screenshot(name) {
  const clip = await evaluate(
    "(()=>{const r=document.querySelector('.n-expanded').getBoundingClientRect();return {x:Math.max(0,r.x),y:Math.max(0,r.y),width:r.width,height:r.height,scale:1}})()",
  );
  const { data } = await cdp("Page.captureScreenshot", { format: "png", clip });
  await writeFile(resolve(output, name + ".png"), Buffer.from(data, "base64"));
}
let previous;
try {
  // Find the actual bundled stores. No fixture snapshot or mocked bridge is injected.
  const assets = (await readdir(resolve(windows, "dist/assets"))).filter((f) => f.endsWith(".js"));
  const candidates = [];
  for (const asset of assets) {
    const text = await readFile(resolve(windows, "dist/assets", asset), "utf8");
    if (text.includes("shownAt:") || text.includes("showingSections:")) candidates.push(asset);
  }
  await evaluate(
    `(async()=>{for(const path of ${JSON.stringify(candidates)}){const m=await import('/assets/'+path);for(const v of Object.values(m)){if(typeof v?.getState!=='function')continue;if('showingSections' in v.getState())window.__issue224Island=v;}}if(!window.__issue224Island)throw Error('Island store not found');const api=window.__TAURI_INTERNALS__;window.__issue224Listener=await api.invoke('plugin:event|listen',{event:'sessions://snapshot',target:{kind:'Any'},handler:api.transformCallback(e=>{window.__issue224Snapshot=e.payload;})});return true})()`,
  );
  previous = await evaluate(
    "({expanded:window.__issue224Island.getState().expanded,selected:window.__issue224Island.getState().selected})",
  );
  await send({ kind: "created" });
  await send({ kind: "title", title: "Issue #224 — harmless security fixture" });
  const short = await ask("short", "  printf '%s\\n' 'SAFE_FIXTURE 🦀'  ");
  assert.equal(await evaluate("document.querySelector('.s-button.is-allow').disabled"), false);
  assert.equal(await evaluate("document.querySelector('.s-button.is-deny').disabled"), false);
  await screenshot("01-short-command");
  await click(".s-card .s-button.is-allow");
  assert.equal((await short.reply).reply, "once");

  const raw = "  " + "# harmless padding\n".repeat(600) + "printf '%s\\n' 'HIDDEN_SUFFIX_FIXTURE 🦀'  ";
  const long = await ask("long", raw);
  const args = { sessionId, requestId: "long", revision: long.p.command.revision };
  assert.equal(await invoke("sessions_decide", { ...args, decision: "allow" }), false);
  assert.equal(await invoke("sessions_decide", { ...args, decision: "always", inspectionToken: "forged" }), false);
  assert.equal(await evaluate("document.querySelector('.s-button.is-allow').disabled"), true);
  assert.equal(await evaluate("document.querySelector('.s-card [aria-keyshortcuts=\"Control+t\"]').disabled"), true);
  assert.equal(await evaluate("document.querySelector('.s-button.is-deny').disabled"), false);
  await screenshot("02-shortened-blocked");
  await cdp("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "y",
    code: "KeyY",
    modifiers: 2,
    windowsVirtualKeyCode: 89,
  });
  await cdp("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "y",
    code: "KeyY",
    modifiers: 2,
    windowsVirtualKeyCode: 89,
  });
  assert.equal((await checkSession()).permission.requestId, "long");
  await click(".s-command-notice button");
  await until(
    () =>
      evaluate("!!document.querySelector('.s-full-command') && !document.querySelector('.s-button.is-allow').disabled"),
    "full viewer enabled",
  );
  assert.equal(await evaluate("document.querySelector('.s-full-command').textContent"), raw);
  assert.equal(await evaluate("document.activeElement===document.querySelector('.s-full-command')"), true);
  assert.equal(await evaluate("document.querySelector('.s-card [aria-keyshortcuts=\"Control+t\"]').disabled"), false);
  await screenshot("03-full-command-top");
  await evaluate("document.querySelector('.s-full-command').scrollTop=1000000;true");
  await pause(200);
  assert(
    await evaluate(
      "(()=>{const p=document.querySelector('.s-full-command');return p.clientHeight>0&&p.scrollTop+p.clientHeight>=p.scrollHeight-1})()",
    ),
    "the complete suffix can actually be scrolled into view",
  );
  await screenshot("04-full-command-suffix-allow-enabled");
  await click(".s-command-notice button");
  assert.equal(await evaluate("document.querySelector('.s-button.is-allow').disabled"), true);
  assert.equal(await invoke("sessions_decide", { ...args, decision: "allow" }), false);
  await click(".s-command-notice button");
  await until(() => evaluate("!!document.querySelector('.s-full-command')"), "reopened viewer");
  await click(".s-card .s-button.is-allow");
  assert.equal((await long.reply).reply, "once");

  const always = await ask("inspected-always", raw);
  await click(".s-command-notice button");
  await until(
    () =>
      evaluate(
        "!!document.querySelector('.s-full-command') && !document.querySelector('.s-card [aria-keyshortcuts=\"Control+t\"]').disabled",
      ),
    "inspected Always",
  );
  await click('.s-card [aria-keyshortcuts="Control+t"]');
  assert.equal((await always.reply).reply, "always");

  const horizontalRaw = "  " + "x".repeat(9000) + " HORIZONTAL_SUFFIX_FIXTURE  ";
  const horizontal = await ask("horizontal", horizontalRaw);
  await click(".s-command-notice button");
  await until(
    () =>
      evaluate("!!document.querySelector('.s-full-command') && !document.querySelector('.s-button.is-allow').disabled"),
    "horizontal viewer",
  );
  assert.equal(await evaluate("document.querySelector('.s-full-command').textContent"), horizontalRaw);
  assert(
    await evaluate(
      "(()=>{const p=document.querySelector('.s-full-command');p.scrollLeft=p.scrollWidth;return p.scrollWidth>p.clientWidth})()",
    ),
    "long lines have a horizontal scroll area",
  );
  await pause(200);
  assert(
    await evaluate(
      "(()=>{const p=document.querySelector('.s-full-command');return p.scrollLeft+p.clientWidth>=p.scrollWidth-1})()",
    ),
    "the horizontal suffix is reachable",
  );
  await screenshot("06-horizontal-suffix-controls-visible");
  await click(".s-card .s-button.is-deny");
  assert.equal((await horizontal.reply).reply, "reject");

  const controls = await ask(
    "controls",
    "  printf '<img src=x onerror=window.__issue224Injected=true>'\r\n\t# \u001b[2J\u202e END  ",
  );
  assert.equal(await evaluate("document.querySelector('.s-button.is-allow').disabled"), true);
  await click(".s-command-notice button");
  await until(
    () =>
      evaluate("!!document.querySelector('.s-full-command') && !document.querySelector('.s-button.is-allow').disabled"),
    "control character viewer",
  );
  assert(
    await evaluate(
      "(()=>{const p=document.querySelector('.s-full-command');return p.textContent.includes('⟦U+001B⟧')&&p.textContent.includes('⟦U+202E⟧')&&!p.querySelector('img')&&!window.__issue224Injected})()",
    ),
    "controls are visible and HTML remains inert text",
  );
  await screenshot("07-controls-and-inert-text");
  await click(".s-card .s-button.is-deny");
  assert.equal((await controls.reply).reply, "reject");

  const incomplete = await ask("legacy", "echo PARTIAL_COMMAND", false);
  assert.equal(await evaluate("document.querySelector('.s-button.is-allow').disabled"), true);
  assert.equal(await evaluate("!!document.querySelector('.s-command-notice button')"), false);
  await screenshot("05-incomplete-deny-enabled");
  await click(".s-card .s-button.is-deny");
  assert.equal((await incomplete.reply).reply, "reject");

  const patternRaw = "\n  " + "x".repeat(9000) + " HIDDEN_PATTERN_SUFFIX \u{1f980}  \n";
  const pattern = await ask("patterns", "", true, {
    permission: "custom",
    metadata: {},
    patterns: ["", patternRaw.slice(1, -1), ""],
    permissionContentComplete: true,
    permissionDetailLength: Array.from(patternRaw).length,
  });
  assert.equal(await evaluate("document.querySelector('.s-button.is-allow').disabled"), true);
  await click(".s-command-notice button");
  await until(
    () =>
      evaluate("!!document.querySelector('.s-full-command') && !document.querySelector('.s-button.is-allow').disabled"),
    "custom patterns viewer",
  );
  assert.equal(await evaluate("document.querySelector('.s-full-command').textContent"), patternRaw);
  await click(".s-card .s-button.is-allow");
  assert.equal((await pattern.reply).reply, "once");
  const legacyPattern = await ask("legacy-pattern", "", true, {
    permission: "custom",
    metadata: {},
    patterns: ["short legacy pattern"],
  });
  assert.equal(await evaluate("document.querySelector('.s-button.is-allow').disabled"), true);
  assert.equal(
    await invoke("sessions_decide", {
      sessionId,
      requestId: "legacy-pattern",
      revision: legacyPattern.p.command.revision,
      decision: "allow",
    }),
    false,
  );
  await click(".s-card .s-button.is-deny");
  assert.equal((await legacyPattern.reply).reply, "reject");

  const mixed = await ask("command-with-diff", "echo FIXTURE", true, {
    permission: "edit",
    metadata: {
      command: "echo FIXTURE",
      commandComplete: true,
      commandLength: 12,
      filepath: "/fixture.txt",
      diff: "--- a/fixture.txt\n+++ b/fixture.txt\n@@ -1 +1 @@\n-old\n+new\n",
    },
  });
  assert.equal(await evaluate("document.querySelector('.s-card .s-detail').textContent"), "echo FIXTURE");
  assert.equal(await evaluate("!!document.querySelector('.s-card .s-diff')"), false);
  await click(".s-card .s-button.is-deny");
  assert.equal((await mixed.reply).reply, "reject");

  const beforeReplacement = await ask("replace-same-id", raw);
  await click(".s-command-notice button");
  await until(
    () =>
      evaluate("!!document.querySelector('.s-full-command') && !document.querySelector('.s-button.is-allow').disabled"),
    "replacement initial viewer",
  );
  const oldProof = await invoke("sessions_inspect_command", {
    sessionId,
    requestId: "replace-same-id",
    revision: beforeReplacement.p.command.revision,
  });
  const changedRaw = raw + "\n# REPLACED_COMMAND_FIXTURE";
  const replacementReply = send({
    kind: "permission",
    id: "replace-same-id",
    permission: "bash",
    metadata: { command: changedRaw, commandComplete: true, commandLength: Array.from(changedRaw).length },
  });
  await until(async () => {
    const current = (await checkSession())?.permission;
    return (
      current?.requestId === "replace-same-id" && current.command?.revision !== beforeReplacement.p.command.revision
    );
  }, "same-id replacement revision");
  await until(
    () =>
      evaluate("!document.querySelector('.s-full-command') && document.querySelector('.s-button.is-allow')?.disabled"),
    "replacement relocks the card",
  );
  assert.equal(
    await invoke("sessions_decide", {
      sessionId,
      requestId: "replace-same-id",
      revision: beforeReplacement.p.command.revision,
      inspectionToken: oldProof.token,
      decision: "always",
    }),
    false,
  );
  await beforeReplacement.reply;
  await click(".s-card .s-button.is-deny");
  assert.equal((await replacementReply).reply, "reject");

  const expired = await ask("expired", raw);
  await click(".s-command-notice button");
  await until(() => evaluate("!!document.querySelector('.s-full-command')"), "expiry viewer");
  const expiring = await invoke("sessions_inspect_command", {
    sessionId,
    requestId: "expired",
    revision: expired.p.command.revision,
  });
  await send({ kind: "permissionReplied", id: "expired" });
  await until(async () => !(await checkSession())?.permission, "request resolved");
  assert.equal(
    await invoke("sessions_decide", {
      sessionId,
      requestId: "expired",
      revision: expired.p.command.revision,
      decision: "allow",
      inspectionToken: expiring.token,
    }),
    false,
  );
  await expired.reply;
  await writeFile(
    resolve(output, "results.json"),
    JSON.stringify(
      {
        passed: true,
        actualTauri: true,
        installedSha256,
        noCommandExecution: true,
        checks: [
          "short Allow",
          "truncated Allow/Always blocked at Rust boundary",
          "keyboard bypass",
          "exact full text and focus",
          "enabled expanded controls",
          "close relocks controls",
          "inspected Always reaches the agent transport",
          "horizontal suffix remains reachable with visible controls",
          "control annotations and inert HTML text",
          "explicit expanded Allow",
          "incomplete Deny",
          "custom patterns preserve full text and require source completeness",
          "expired request rejects inspection",
          "file diffs cannot hide an authoritative command",
          "same-id command replacement closes the viewer and rejects old proof",
        ],
      },
      null,
      2,
    ),
  );
  console.log(`Actual Tauri security checks passed. Screenshots: ${output}`);
} finally {
  await send({ kind: "deleted" }).catch(() => {});
  if (previous)
    await evaluate(
      `${previous.expanded ? `window.__issue224Island.getState().open(${JSON.stringify(previous.selected)});` : "window.__issue224Island.getState().collapse();"}window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten',{event:'sessions://snapshot',eventId:window.__issue224Listener});true`,
    ).catch(() => {});
  for (const socket of sockets) socket.destroy();
  ws.close();
}
