import { EventEmitter } from "node:events";
import { readFileSync } from "node:fs";
import { beforeEach, expect, it, vi } from "vitest";

const { sent, sockets, transport } = vi.hoisted(() => ({ sent: [], sockets: [], transport: { hold: false } }));
vi.mock("node:net", () => ({
  connect() {
    const socket = new EventEmitter();
    socket.write = (data) => {
      const payload = JSON.parse(data);
      sent.push(payload.event);
      sockets.push({ socket, event: payload.event });
      // Leave listen pending without scheduling timers or connecting anywhere.
      if (payload.event.kind !== "listen" && !(transport.hold && payload.event.kind === "permission"))
        queueMicrotask(() => socket.emit("close"));
    };
    socket.end = socket.write;
    socket.setTimeout = () => socket;
    socket.destroy = () => socket.emit("close");
    queueMicrotask(() => socket.emit("connect"));
    return socket;
  },
}));

import { OpenbarIsland } from "../src-tauri/src/sessions/opencode-plugin.js";

beforeEach(() => {
  sent.length = 0;
  sockets.length = 0;
  transport.hold = false;
});

it.each([3999, 4000, 4001, 8000, 8001, 50000])(
  "OpenCode sends all %i characters, including suffix and whitespace",
  async (length) => {
    const command = `  ${"x".repeat(length)}\n# HIDDEN_SUFFIX_FIXTURE 🦀  `;
    const client = { postSessionIdPermissionsPermissionId: vi.fn() };
    const plugin = await OpenbarIsland({ client, directory: "/fixture" });
    await plugin.event({
      event: {
        type: "permission.asked",
        properties: { sessionID: "s", id: "p", permission: "bash", metadata: { command } },
      },
    });
    await vi.waitFor(() => expect(sent.some((e) => e.kind === "permission")).toBe(true));
    const p = sent.find((e) => e.kind === "permission");
    expect(p.metadata.command).toBe(command);
    expect(p.metadata.commandComplete).toBe(true);
    expect(p.metadata.commandLength).toBe(Array.from(command).length);
    expect(client.postSessionIdPermissionsPermissionId).not.toHaveBeenCalled();
  },
);

it.each([undefined, 42, {}, ""])("malformed/empty OpenCode command %j is explicitly incomplete", async (command) => {
  const plugin = await OpenbarIsland({ client: {}, directory: "/fixture" });
  await plugin.event({
    event: {
      type: "permission.asked",
      properties: { sessionID: "s", id: "p", permission: "bash", metadata: { command } },
    },
  });
  await vi.waitFor(() => expect(sent.some((e) => e.kind === "permission")).toBe(true));
  expect(sent.find((e) => e.kind === "permission").metadata.commandComplete).toBe(false);
});

it("oversized commands explicitly report missing content and original length", async () => {
  const command = "x".repeat(1048577);
  const plugin = await OpenbarIsland({ client: {}, directory: "/fixture" });
  await plugin.event({
    event: {
      type: "permission.asked",
      properties: { sessionID: "s", id: "p", permission: "bash", metadata: { command } },
    },
  });
  await vi.waitFor(() => expect(sent.some((e) => e.kind === "permission")).toBe(true));
  const metadata = sent.find((e) => e.kind === "permission").metadata;
  expect(metadata.commandComplete).toBe(false);
  expect(metadata.commandLength).toBe(command.length);
  expect(metadata.command.length).toBe(8000);
});

it("macOS embeds exactly the same tested OpenCode plugin", () => {
  const windows = readFileSync(
    new URL("../src-tauri/src/sessions/opencode-plugin.js", import.meta.url),
    "utf8",
  ).replaceAll("\r\n", "\n");
  const mac = readFileSync(
    new URL("../../Sources/Openbar/Services/ClaudeSessions/AgentOpenCodePlugin.swift", import.meta.url),
    "utf8",
  ).replaceAll("\r\n", "\n");
  expect(mac).toContain(windows);
});

it.each([3999, 4000, 4001, 8001, 50000])(
  "custom permission patterns preserve all %i characters and empty lines",
  async (length) => {
    const raw = "  " + "x".repeat(length) + " HIDDEN_PATTERN_SUFFIX 🦀  ";
    const patterns = ["", raw, ""];
    const plugin = await OpenbarIsland({ client: {}, directory: "/fixture" });
    await plugin.event({
      event: {
        type: "permission.asked",
        properties: { sessionID: "s", id: "custom", permission: "custom_shell", patterns },
      },
    });
    await vi.waitFor(() => expect(sent.some((e) => e.kind === "permission")).toBe(true));
    const permission = sent.find((e) => e.kind === "permission");
    expect(permission.patterns).toEqual(patterns);
    expect(permission.permissionContentComplete).toBe(true);
    expect(permission.permissionDetailLength).toBe(Array.from(patterns.join("\n")).length);
  },
);

it("fallback metadata preserves unknown fields and paths without the old 4000-character cut", async () => {
  const metadata = { script: "x".repeat(9000) + " HIDDEN_METADATA_SUFFIX", filepath: "z".repeat(5000) };
  const plugin = await OpenbarIsland({ client: {}, directory: "/fixture" });
  await plugin.event({
    event: {
      type: "permission.asked",
      properties: { sessionID: "s", id: "metadata", permission: "custom_shell", metadata },
    },
  });
  await vi.waitFor(() => expect(sent.some((e) => e.kind === "permission")).toBe(true));
  const permission = sent.find((e) => e.kind === "permission");
  expect(permission.permissionContentComplete).toBe(true);
  expect(permission.metadata.script).toBe(metadata.script);
  expect(permission.metadata.filepath).toBe(metadata.filepath);
});

it.each([
  { patterns: ["x".repeat(1048577)] },
  { patterns: ["command", null] },
  { patterns: "malformed pattern array", metadata: { filepath: "short" } },
  { metadata: { script: "x".repeat(1048577) } },
  { metadata: 42 },
  { metadata: {} },
])("oversized or malformed custom permissions are explicitly incomplete", async (fields) => {
  const plugin = await OpenbarIsland({ client: {}, directory: "/fixture" });
  await plugin.event({
    event: {
      type: "permission.asked",
      properties: { sessionID: "s", id: "incomplete", permission: "custom_shell", ...fields },
    },
  });
  await vi.waitFor(() => expect(sent.some((e) => e.kind === "permission")).toBe(true));
  expect(sent.find((e) => e.kind === "permission").permissionContentComplete).toBe(false);
});

it.each(["once", "always"])("changed OpenCode requests reject queued stale %s replies", async (response) => {
  transport.hold = true;
  const client = { postSessionIdPermissionsPermissionId: vi.fn() };
  const plugin = await OpenbarIsland({ client, directory: "/fixture" });
  const properties = { sessionID: "s", id: "same-id", permission: "bash", metadata: { command: "echo OLD_FIXTURE" } };
  await plugin.event({ event: { type: "permission.asked", properties } });
  await vi.waitFor(() => expect(sockets.filter((s) => s.event.kind === "permission")).toHaveLength(1));
  const old = sockets.find((s) => s.event.kind === "permission").socket;
  // Resolve the transport, but replace the command before ask's continuation forwards it.
  old.emit("data", JSON.stringify({ reply: response }));
  old.emit("close");
  const replacement = { ...properties, metadata: { command: "echo NEW_FIXTURE " + "x".repeat(9000) } };
  await plugin.event({ event: { type: "permission.updated", properties: replacement } });
  await vi.waitFor(() => expect(sockets.filter((s) => s.event.kind === "permission")).toHaveLength(2));
  expect(client.postSessionIdPermissionsPermissionId).not.toHaveBeenCalled();
  old.emit("close");
  await plugin.event({ event: { type: "permission.updated", properties: replacement } });
  expect(sent.filter((e) => e.kind === "permission")).toHaveLength(2);
  const current = sockets.filter((s) => s.event.kind === "permission")[1];
  expect(current.event.metadata.command).toBe(replacement.metadata.command);
  current.socket.emit("data", JSON.stringify({ reply: response }));
  current.socket.emit("close");
  await vi.waitFor(() => expect(client.postSessionIdPermissionsPermissionId).toHaveBeenCalledTimes(1));
  expect(client.postSessionIdPermissionsPermissionId).toHaveBeenLastCalledWith({
    path: { id: "s", permissionID: "same-id" },
    body: { response },
  });
});

it.each(["permission.replied", "session.deleted"])("%s invalidates a queued OpenCode approval", async (type) => {
  transport.hold = true;
  const client = { postSessionIdPermissionsPermissionId: vi.fn() };
  const plugin = await OpenbarIsland({ client, directory: "/fixture" });
  const properties = { sessionID: "s", id: "cancelled", permission: "bash", metadata: { command: "echo FIXTURE" } };
  await plugin.event({ event: { type: "permission.asked", properties } });
  await vi.waitFor(() => expect(sockets.some((s) => s.event.kind === "permission")).toBe(true));
  const old = sockets.find((s) => s.event.kind === "permission").socket;
  old.emit("data", JSON.stringify({ reply: "always" }));
  old.emit("close");
  await plugin.event({ event: { type, properties } });
  await Promise.resolve();
  expect(client.postSessionIdPermissionsPermissionId).not.toHaveBeenCalled();
});
