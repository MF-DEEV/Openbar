// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { beforeEach, expect, it, vi } from "vitest";
import { useIsland } from "../island/store";
import { demoSnapshot } from "./demo";
import { handlePermissionKey } from "./permissionKeys";
import { useSessions } from "./store";

const decide = vi.fn(async () => true);
const session = demoSnapshot().sessions[0]!;

function key(value: string, changes: Partial<KeyboardEvent> = {}) {
  return {
    key: value,
    ctrlKey: true,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    repeat: false,
    isComposing: false,
    defaultPrevented: false,
    target: null,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    ...changes,
  } as unknown as KeyboardEvent;
}

beforeEach(() => {
  decide.mockClear();
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

it.each([
  ["y", "allow"],
  ["Y", "allow"],
  ["n", "deny"],
])("Ctrl+%s answers the visible request", (value, decision) => {
  const event = key(value!);
  expect(handlePermissionKey(event, session.id, session.permission!.requestId)).toBe(true);
  expect(decide).toHaveBeenCalledExactlyOnceWith(session, decision);
  expect(event.preventDefault).toHaveBeenCalledOnce();
});

it("keeps the settling interval and ignores keys held down", () => {
  useSessions.setState({ shownAt: { [session.permission!.requestId]: Date.now() / 1000 } });
  expect(handlePermissionKey(key("y"), session.id, session.permission!.requestId)).toBe(true);
  expect(handlePermissionKey(key("n", { repeat: true }), session.id, session.permission!.requestId)).toBe(false);
  expect(decide).not.toHaveBeenCalled();
});

it("does not answer a replaced request or a hidden card", () => {
  expect(handlePermissionKey(key("y"), session.id, "old-request")).toBe(false);
  useIsland.setState({ showingSections: true });
  expect(handlePermissionKey(key("y"), session.id, session.permission!.requestId)).toBe(false);
  useIsland.setState({ showingSections: false, expanded: false });
  expect(handlePermissionKey(key("n"), session.id, session.permission!.requestId)).toBe(false);
  expect(decide).not.toHaveBeenCalled();
});

it("leaves editor shortcuts, composition and other modifiers alone", () => {
  const editor = { closest: () => ({}) } as unknown as EventTarget;
  for (const changes of [
    { target: editor },
    { isComposing: true },
    { ctrlKey: false },
    { altKey: true },
    { metaKey: true },
  ]) {
    expect(handlePermissionKey(key("y", changes), session.id, session.permission!.requestId)).toBe(false);
  }
  expect(decide).not.toHaveBeenCalled();
});

it("Ctrl+T selects Always and enables continuous approvals only for the displayed Codex session", () => {
  expect(handlePermissionKey(key("t"), session.id, session.permission!.requestId)).toBe(true);
  expect(decide).toHaveBeenCalledExactlyOnceWith(session, "always");
  decide.mockClear();
  const codex = { ...session, provider: "codex" as const };
  useSessions.setState({ snapshot: { ...demoSnapshot(), sessions: [codex] }, autoAllow: {} });
  expect(handlePermissionKey(key("T"), codex.id, codex.permission!.requestId)).toBe(true);
  expect(useSessions.getState().autoAllow[codex.id]).toBe(true);
  expect(decide).not.toHaveBeenCalled();
  expect(handlePermissionKey(key("n", { shiftKey: true }), codex.id, codex.permission!.requestId)).toBe(false);
});

it("does not approve a question form or answer under the wardrobe", () => {
  useSessions.setState({ wardrobe: true });
  expect(handlePermissionKey(key("y"), session.id, session.permission!.requestId)).toBe(false);
  useSessions.setState({ wardrobe: false, snapshot: demoSnapshot(), focused: "b2" });
  expect(handlePermissionKey(key("y"), "b2", "r2")).toBe(false);
  expect(decide).not.toHaveBeenCalled();
});
