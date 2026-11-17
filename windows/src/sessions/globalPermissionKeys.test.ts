// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ShortcutEvent } from "@tauri-apps/plugin-global-shortcut";
import { bridge } from "../shared/bridge";
import { useIsland } from "../island/store";
import { demoSnapshot } from "./demo";
import { subscribeGlobalPermissionKeys } from "./globalPermissionKeys";
import { useSessions } from "./store";

const handlers = new Map<string, (event: ShortcutEvent) => void>();
const session = demoSnapshot().sessions[0]!;
const decide = vi.fn(async () => true);
let cleanup: (() => void) | undefined;
let focused = false;
let editor = false;
let fullscreen: (hidden: boolean) => void;
let page: EventTarget;

const flush = async () => {
  for (let i = 0; i < 30; i++) await Promise.resolve();
};
const press = (key: string, state: "Pressed" | "Released" = "Pressed") =>
  handlers.get(key)?.({ shortcut: `Control+${key}`, id: 1, state });

beforeEach(() => {
  handlers.clear();
  decide.mockClear();
  focused = false;
  editor = false;
  page = new EventTarget();
  Object.assign(page, {
    hasFocus: () => focused,
    activeElement: { closest: () => (editor ? {} : null) },
    querySelector: () => null,
  });
  vi.stubGlobal("document", page);
  vi.stubGlobal("window", new EventTarget());
  vi.spyOn(bridge.sessions, "registerPermissionKey").mockImplementation(async (key, handler) => {
    handlers.set(key, handler);
  });
  vi.spyOn(bridge.sessions, "unregisterPermissionKey").mockImplementation(async (key) => {
    handlers.delete(key);
  });
  vi.spyOn(bridge, "onFullscreen").mockImplementation(async (handler) => {
    fullscreen = handler;
    return () => {};
  });
  useIsland.setState({ expanded: true, selected: "sessions", showingSections: false });
  useSessions.setState({
    snapshot: { ...demoSnapshot(), sessions: [session] },
    focused: session.id,
    shownAt: { [session.permission!.requestId]: Date.now() / 1000 - 2 },
    renaming: false,
    wardrobe: false,
    decide,
  });
});

afterEach(async () => {
  cleanup?.();
  cleanup = undefined;
  await flush();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it.each([
  ["y", "allow"],
  ["n", "deny"],
])("global Ctrl+%s answers with another app focused", async (key, decision) => {
  // A stale editor focus in the background webview must not block the shortcut.
  editor = true;
  cleanup = subscribeGlobalPermissionKeys(session.id, session.permission!.requestId);
  await flush();
  press(key!);
  press(key!);
  expect(decide).toHaveBeenCalledExactlyOnceWith(session, decision);
  await flush();
  press(key!);
  expect(decide).toHaveBeenCalledTimes(1);
});

it("releases both shortcuts when the card closes and ignores queued events", async () => {
  cleanup = subscribeGlobalPermissionKeys(session.id, session.permission!.requestId);
  await flush();
  const old = handlers.get("y")!;
  useIsland.setState({ expanded: false });
  old({ shortcut: "Control+Y", id: 1, state: "Pressed" });
  await flush();
  expect(handlers.size).toBe(0);
  expect(decide).not.toHaveBeenCalled();
});

it("global Ctrl+T enables Always for the visible Codex session", async () => {
  const codex = { ...session, provider: "codex" as const };
  useSessions.setState({ snapshot: { ...demoSnapshot(), sessions: [codex] }, autoAllow: {} });
  cleanup = subscribeGlobalPermissionKeys(codex.id, codex.permission!.requestId);
  await flush();
  press("always");
  press("always");
  expect(useSessions.getState().autoAllow[codex.id]).toBe(true);
  expect(decide).not.toHaveBeenCalled();
});

it("keeps the settling delay and rejects replaced requests", async () => {
  useSessions.setState({ shownAt: { [session.permission!.requestId]: Date.now() / 1000 } });
  cleanup = subscribeGlobalPermissionKeys(session.id, session.permission!.requestId);
  await flush();
  press("y");
  expect(decide).not.toHaveBeenCalled();
  useSessions.setState({ snapshot: { ...demoSnapshot(), sessions: [] } });
  press("y", "Released");
  press("n");
  await flush();
  expect(handlers.size).toBe(0);
  expect(decide).not.toHaveBeenCalled();
});

it("returns editor shortcuts while typing in the island and restores them on blur", async () => {
  cleanup = subscribeGlobalPermissionKeys(session.id, session.permission!.requestId);
  await flush();
  focused = true;
  editor = true;
  page.dispatchEvent(new Event("focusin"));
  await flush();
  expect(handlers.size).toBe(0);
  focused = false;
  window.dispatchEvent(new Event("blur"));
  await flush();
  expect(handlers.size).toBe(3);
});

it("unregisters while fullscreen hides the island", async () => {
  cleanup = subscribeGlobalPermissionKeys(session.id, session.permission!.requestId);
  await flush();
  fullscreen(true);
  await flush();
  expect(handlers.size).toBe(0);
  fullscreen(false);
  await flush();
  expect(handlers.size).toBe(3);
});

it("cleans up registrations that finish after unmount before registering the next card", async () => {
  let release!: () => void;
  vi.mocked(bridge.sessions.registerPermissionKey).mockImplementationOnce(async (key, handler) => {
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    handlers.set(key, handler);
  });
  const first = subscribeGlobalPermissionKeys(session.id, session.permission!.requestId);
  await flush();
  first();
  cleanup = subscribeGlobalPermissionKeys(session.id, session.permission!.requestId);
  release();
  await flush();
  expect(handlers.size).toBe(3);
  press("n");
  expect(decide).toHaveBeenCalledExactlyOnceWith(session, "deny");
});
