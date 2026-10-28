// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's view of Claude Code sessions: the latest snapshot from the
// Rust service, which session page is open, drafts kept while the island
// closes, and the actions that go back to Rust.

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import { localCostumeStore } from "./clawd/costume";
import type { Decision, ReplyAttachment, ReplyFailure, Session, SessionEvent, Snapshot } from "./types";
import { DEFAULT_SESSION_PREFS, type SessionPrefs } from "../shared/prefs";

/** A permission must be on screen this long before a click or Ctrl+Y can answer it. */
export const SETTLE_DELAY = 0.8;
export const REPLY_LIMIT = 4000;
export const OWN_ANSWER_LIMIT = 7500;

interface SessionsState {
  snapshot: Snapshot;
  /** The session whose page is open, if any. */
  focused: string | null;
  /** When each permission request first showed, by request id. */
  shownAt: Record<string, number>;
  drafts: Record<string, string>;
  attachments: Record<string, ReplyAttachment[]>;
  names: Record<string, string>;
  lookedAt: Record<string, number>;
  renaming: boolean;
  /** The wardrobe is open on the focused session's page. */
  wardrobe: boolean;
  /** Explicit opt-in for the current Codex turn; cleared when it ends. */
  autoAllow: Record<string, boolean>;
  toggleAutoAllow(id: string): void;
  /** The section's preferences (Settings › Content › Sessions). */
  prefs: SessionPrefs;
  /** Outfits changed in the wardrobe, by session (or subagent) key. */
  costumes: Record<string, string>;
  setCostume(key: string, code: string | null): void;
  /** "Couldn’t find that window" shows until this time. */
  openFailedUntil: number;
  receive(snapshot: Snapshot): void;
  focus(id: string | null): void;
  setDraft(id: string, text: string): void;
  addAttachments(id: string, images: ReplyAttachment[]): void;
  removeAttachment(id: string, path: string): void;
  rename(id: string, name: string): void;
  decide(session: Session, decision: Decision): Promise<boolean>;
  answer(session: Session, answers: Record<string, string | string[]>): Promise<boolean>;
  skip(session: Session): Promise<boolean>;
  reply(session: Session, text: string, images: string[]): Promise<ReplyFailure | null>;
  openWindow(session: Session): Promise<boolean>;
}

const EMPTY: Snapshot = {
  sessions: [],
  counts: { working: 0, workingCodex: 0, waiting: 0, finished: 0 },
  previews: false,
  hooksInstalled: false,
};

const now = () => Date.now() / 1000;
const costumeStore = localCostumeStore();

function loadNames(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem("sessions.names") ?? "{}");
  } catch {
    return {};
  }
}

/** The request is still the one shown for that session, and has been on screen long enough. */
export function settled(state: Pick<SessionsState, "shownAt">, session: Session, at = now()): boolean {
  const p = session.permission;
  if (!p) return false;
  const shown = state.shownAt[p.requestId];
  return shown !== undefined && at - shown >= SETTLE_DELAY;
}

export const useSessions = create<SessionsState>()((set, get) => ({
  snapshot: EMPTY,
  focused: null,
  shownAt: {},
  drafts: {},
  attachments: {},
  names: loadNames(),
  lookedAt: {},
  renaming: false,
  wardrobe: false,
  autoAllow: {},
  toggleAutoAllow(id) {
    if (
      !get().snapshot.sessions.some(
        (s) => s.id === id && s.provider === "codex" && s.phase !== "finished" && s.phase !== "idle",
      )
    )
      return;
    set((s) => ({ autoAllow: { ...s.autoAllow, [id]: !s.autoAllow[id] } }));
  },
  prefs: DEFAULT_SESSION_PREFS,
  costumes: costumeStore.all(),
  openFailedUntil: 0,

  setCostume(key, code) {
    const live = new Set(get().snapshot.sessions.map((x) => x.id));
    costumeStore.set(key, code, live);
    set({ costumes: costumeStore.all() });
  },

  receive(snapshot) {
    const previous = get();
    // Expire consent before subscribers can schedule approvals for a new turn.
    // Comparing turn starts also covers a completion missed between snapshots.
    const autoAllow = Object.fromEntries(
      Object.entries(previous.autoAllow).filter(([id, enabled]) => {
        if (!enabled) return false;
        const session = snapshot.sessions.find((s) => s.id === id);
        const before = previous.snapshot.sessions.find((s) => s.id === id);
        return (
          session?.provider === "codex" &&
          session.phase !== "finished" &&
          session.phase !== "idle" &&
          !!before &&
          (before.turnStarted === undefined ||
            session.turnStarted === undefined ||
            before.turnStarted === session.turnStarted)
        );
      }),
    );
    const shownAt: Record<string, number> = {};
    const t = now();
    for (const s of snapshot.sessions) {
      const id = s.permission?.requestId;
      if (id) shownAt[id] = get().shownAt[id] ?? t;
    }
    set({ snapshot, shownAt, autoAllow });
  },

  focus(id) {
    set((s) => ({
      focused: id,
      renaming: false,
      wardrobe: false,
      lookedAt: id ? { ...s.lookedAt, [id]: now() } : s.lookedAt,
    }));
    if (id) void bridge.sessions.acknowledge(id);
    void bridge.sessions.focus(id);
  },

  setDraft: (id, text) => set((s) => ({ drafts: { ...s.drafts, [id]: text.slice(0, REPLY_LIMIT) } })),

  addAttachments: (id, images) =>
    set((s) => ({
      attachments: {
        ...s.attachments,
        [id]: [...(s.attachments[id] ?? []), ...images].slice(0, 5),
      },
    })),

  removeAttachment: (id, path) =>
    set((s) => ({
      attachments: {
        ...s.attachments,
        [id]: (s.attachments[id] ?? []).filter((image) => image.path !== path),
      },
    })),

  rename(id, name) {
    const trimmed = name.trim().slice(0, 60);
    set((s) => {
      const names = { ...s.names };
      if (trimmed) names[id] = trimmed;
      else delete names[id];
      // At most 100 kept; names of sessions no longer running go first.
      const running = new Set(s.snapshot.sessions.map((x) => x.id));
      const keys = Object.keys(names);
      for (const key of keys.filter((k) => !running.has(k))) {
        if (Object.keys(names).length <= 100) break;
        delete names[key];
      }
      try {
        localStorage.setItem("sessions.names", JSON.stringify(names));
      } catch {
        // Names are a convenience; losing them is fine.
      }
      return { names };
    });
  },

  async decide(session, decision) {
    if (!session.permission || session.permission.questions.length > 0 || !settled(get(), session)) return false;
    return (await bridge.sessions.decide(session.id, session.permission.requestId, decision)) ?? false;
  },

  async answer(session, answers) {
    const p = session.permission;
    if (!p || p.questions.length === 0 || !settled(get(), session)) return false;
    return (await bridge.sessions.answer(session.id, p.requestId, answers)) ?? false;
  },

  async skip(session) {
    const p = session.permission;
    if (!p || !settled(get(), session)) return false;
    return (await bridge.sessions.decide(session.id, p.requestId, "deny")) ?? false;
  },

  async reply(session, text, images) {
    const body = text.trim().slice(0, REPLY_LIMIT);
    if (!body && images.length === 0) return null;
    // undefined means no app answered at all (a plain browser).
    const result = await bridge.sessions.reply(session.id, body, images);
    const failure = result === undefined ? "unreachable" : result;
    if (failure === null)
      set((s) => ({ drafts: { ...s.drafts, [session.id]: "" }, attachments: { ...s.attachments, [session.id]: [] } }));
    return failure;
  },

  async openWindow(session) {
    const ok = (await bridge.sessions.open(session.id)) ?? false;
    if (!ok) set({ openFailedUntil: now() + 3 });
    else void bridge.sessions.acknowledge(session.id);
    return ok;
  },
}));

export function subscribeSessions(onEvent: (e: SessionEvent) => void): () => void {
  const a = bridge.sessions.onSnapshot((s) => useSessions.getState().receive(s));
  const b = bridge.sessions.onEvent(onEvent);
  return () => {
    void a.then((f) => f());
    void b.then((f) => f());
  };
}
