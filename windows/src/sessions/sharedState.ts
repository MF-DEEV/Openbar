// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// What the person makes of the sessions by hand — a reply being written, the
// pictures added to it, names, Always for a Codex turn, outfits, what was
// looked at — stays the same in the island and in the tray's panel (#331).
// Each window keeps its own store; a change in one reaches the other over an
// event, and the panel, when it opens, asks the island for all of it,
// together with the sessions as they stand, since the backend only sends a
// reading when something changes.

import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { inTauri } from "../shared/bridge";
import { useSessions } from "./store";
import type { Snapshot } from "./types";

export const SHARED_KEYS = ["drafts", "attachments", "names", "autoAllow", "costumes", "lookedAt"] as const;
export type SharedKey = (typeof SHARED_KEYS)[number];
type SessionsState = ReturnType<typeof useSessions.getState>;
export type SharedState = Pick<SessionsState, SharedKey>;

export interface SharedMessage {
  /** The window that sent it, so it never takes its own change back. */
  origin: string;
  patch?: Partial<SharedState>;
  /** The panel opening asks for everything. */
  hello?: boolean;
  /** The sessions as they stand, with the answer to a hello. */
  snapshot?: Snapshot;
}

export interface SharedTransport {
  send(message: SharedMessage): void;
  listen(handler: (message: SharedMessage) => void): Promise<UnlistenFn>;
}

const EVENT = "sessions-ui://shared";

const tauriTransport: SharedTransport = {
  send: (message) => void (inTauri ? emit(EVENT, message) : undefined),
  listen: (handler) =>
    inTauri ? listen<SharedMessage>(EVENT, (event) => handler(event.payload)) : Promise.resolve(() => {}),
};

export function pickShared(state: SharedState): SharedState {
  return Object.fromEntries(SHARED_KEYS.map((key) => [key, state[key]])) as SharedState;
}

/** The shared keys that are not the same object as before, or null when none changed. */
export function changedShared(next: SharedState, previous: SharedState): Partial<SharedState> | null {
  const patch: Partial<SharedState> = {};
  let any = false;
  for (const key of SHARED_KEYS) {
    if (next[key] !== previous[key]) {
      (patch as Record<string, unknown>)[key] = next[key];
      any = true;
    }
  }
  return any ? patch : null;
}

/**
 * Keeps this window's shared state in step with the other's. The island
 * answers a hello with everything it holds; the panel says hello as it
 * starts.
 */
export function startSharedState(origin: "island" | "panel", transport: SharedTransport = tauriTransport): () => void {
  let applying = false;
  // The island's reading fills a panel that has none yet, never a newer one.
  const initial = useSessions.getState().snapshot;
  const stop = useSessions.subscribe((state, previous) => {
    if (applying) return;
    const patch = changedShared(state, previous);
    if (patch) transport.send({ origin, patch });
  });
  const listening = transport.listen((message) => {
    if (message.origin === origin) return;
    if (message.hello) {
      if (origin === "island") {
        const state = useSessions.getState();
        transport.send({ origin, patch: pickShared(state), snapshot: state.snapshot });
      }
      return;
    }
    applying = true;
    try {
      if (message.snapshot && origin === "panel" && useSessions.getState().snapshot === initial)
        useSessions.getState().receive(message.snapshot);
      if (message.patch) useSessions.setState(message.patch);
    } finally {
      applying = false;
    }
  });
  if (origin === "panel") void listening.then(() => transport.send({ origin, hello: true }));
  return () => {
    stop();
    void listening.then((unlisten) => unlisten());
  };
}
