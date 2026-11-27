// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors
import { afterEach, beforeEach, expect, it } from "vitest";
import { demoSnapshot } from "./demo";
import { changedShared, pickShared, startSharedState, type SharedMessage, type SharedTransport } from "./sharedState";
import { useSessions } from "./store";

function bus() {
  const sent: SharedMessage[] = [];
  let handler: ((message: SharedMessage) => void) | null = null;
  const transport: SharedTransport = {
    send: (message) => void sent.push(message),
    listen: async (h) => {
      handler = h;
      return () => {
        handler = null;
      };
    },
  };
  return { transport, sent, deliver: (message: SharedMessage) => handler?.(message) };
}

const initial = useSessions.getState();
let stop: (() => void) | null = null;

beforeEach(() => {
  useSessions.setState({ drafts: {}, attachments: {}, names: {}, autoAllow: {}, costumes: {}, lookedAt: {} });
});
afterEach(() => {
  stop?.();
  stop = null;
  useSessions.setState({ snapshot: initial.snapshot });
});

it("finds what changed among the shared keys", () => {
  const before = pickShared(useSessions.getState());
  expect(changedShared(before, before)).toBeNull();
  const after = { ...before, drafts: { a: "hello" } };
  expect(changedShared(after, before)).toEqual({ drafts: { a: "hello" } });
});

it("sends a change made here and takes the other window's without sending it back", async () => {
  const { transport, sent, deliver } = bus();
  stop = startSharedState("island", transport);
  await Promise.resolve();
  useSessions.setState({ drafts: { s1: "half a reply" } });
  expect(sent).toEqual([{ origin: "island", patch: { drafts: { s1: "half a reply" } } }]);
  deliver({ origin: "panel", patch: { autoAllow: { s2: true } } });
  expect(useSessions.getState().autoAllow).toEqual({ s2: true });
  expect(sent).toHaveLength(1);
  // Its own message, coming back over the event, changes nothing.
  deliver({ origin: "island", patch: { autoAllow: {} } });
  expect(useSessions.getState().autoAllow).toEqual({ s2: true });
});

it("answers the panel's hello with everything, the sessions included", async () => {
  const { transport, sent, deliver } = bus();
  stop = startSharedState("island", transport);
  await Promise.resolve();
  useSessions.setState({ names: { s1: "Checkout" } });
  sent.length = 0;
  deliver({ origin: "panel", hello: true });
  expect(sent).toHaveLength(1);
  expect(sent[0]!.patch?.names).toEqual({ s1: "Checkout" });
  expect(sent[0]!.snapshot).toBe(useSessions.getState().snapshot);
});

it("fills a panel that has no reading yet, never one with a newer reading", async () => {
  const { transport, sent, deliver } = bus();
  stop = startSharedState("panel", transport);
  await Promise.resolve();
  await Promise.resolve();
  expect(sent).toContainEqual({ origin: "panel", hello: true });
  const island = demoSnapshot();
  deliver({ origin: "island", patch: { names: { a1: "Checkout" } }, snapshot: island });
  expect(useSessions.getState().snapshot.sessions.map((s) => s.id)).toEqual(island.sessions.map((s) => s.id));
  expect(useSessions.getState().names).toEqual({ a1: "Checkout" });
  const older = { ...island, sessions: [] };
  deliver({ origin: "island", snapshot: older });
  expect(useSessions.getState().snapshot.sessions).toHaveLength(island.sessions.length);
});
