// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import type { Session, SessionEvent } from "./types";

/**
 * Whether a sessions notice leaves when the person turns to an app: only one
 * about a session still waiting does, and a finished turn's stays
 * (NotchService.swift:4399-4405). A notice with no session always leaves.
 */
export function noticeLeavesWhenBusy(about: string | undefined, sessions: readonly Session[]): boolean {
  return !about || sessions.some((session) => session.id === about && session.phase.startsWith("waiting"));
}

/** Defer pending prompts while another app is in use; never replay resolved requests. */
export class DeferredSessionAttention {
  private pending = new Map<string, { event: SessionEvent; request: string }>();
  private presented = new Map<string, string>();
  constructor(
    private allowed: () => Promise<boolean>,
    private request: (event: SessionEvent) => string | undefined,
    private present: (event: SessionEvent) => void,
  ) {}
  async handle(event: SessionEvent) {
    const request = this.request(event);
    if (event.kind !== "finished" && request === undefined) return;
    const allowed = await this.allowed().catch(() => false);
    if (event.kind !== "finished" && this.request(event) !== request) return;
    if (!allowed) {
      if (event.kind !== "finished") {
        this.pending.set(event.id, { event, request: request! });
        this.presented.delete(event.id);
      }
      return;
    }
    this.pending.delete(event.id);
    if (event.kind !== "finished") {
      if (this.presented.get(event.id) === request) return;
      this.presented.set(event.id, request!);
    }
    this.present(event);
  }
  /** Snapshots are authoritative even if an event was missed or the app restarted. */
  async sync(events: SessionEvent[]) {
    const live = new Set(events.map((event) => event.id));
    for (const id of this.pending.keys()) if (!live.has(id)) this.pending.delete(id);
    for (const id of this.presented.keys()) if (!live.has(id)) this.presented.delete(id);
    await Promise.all(events.map((event) => this.handle(event)));
  }
  async resume() {
    const pending = [...this.pending.values()];
    this.pending.clear();
    for (const { event, request } of pending) {
      if (this.request(event) === request) await this.handle(event);
    }
  }
}
