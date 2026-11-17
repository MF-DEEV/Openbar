// SPDX-License-Identifier: GPL-3.0-or-later
import { create } from "zustand";
import { bridge } from "../shared/bridge";

export interface QueueItem {
  id?: string;
  title: string;
  artist: string;
}
export interface QueueSnapshot {
  source: string;
  title: string;
  status: "ready" | "unsupported" | "unavailable";
  items: QueueItem[];
  prefetch?: QueueItem[];
}
type Load = (source: string, title: string, open: boolean) => Promise<QueueSnapshot | undefined>;
export const QUEUE_HEIGHT = 282;

const songIdentity = (item: QueueItem) => JSON.stringify([item.title, item.artist]);

export function createQueueStore(load: Load) {
  let generation = 0;
  let nextId = 0;
  let lastGoodAt = 0;
  function reconcile(previous: QueueItem[], incoming: QueueItem[]) {
    const pool = [...previous];
    return incoming.slice(0, 5).map((item) => {
      const index = pool.findIndex((old) => songIdentity(old) === songIdentity(item));
      const old = index >= 0 ? pool.splice(index, 1)[0] : undefined;
      return { ...item, id: old?.id ?? `queue-${++nextId}` };
    });
  }
  return create<{
    expanded: boolean;
    loading: boolean;
    recovering: boolean;
    snapshot: QueueSnapshot | null;
    toggle(): void;
    close(): void;
    cancel(): void;
    refresh(source: string, title: string, open?: boolean): Promise<void>;
  }>((set, get) => ({
    expanded: false,
    loading: false,
    recovering: false,
    snapshot: null,
    toggle() {
      if (get().expanded) get().close();
      else set({ expanded: true });
    },
    close() {
      generation++;
      set({ expanded: false, loading: false, recovering: false, snapshot: null });
    },
    cancel() {
      generation++;
      set({ loading: false, recovering: false, snapshot: null });
    },
    async refresh(source, title, open = false) {
      const request = ++generation;
      const previous = get().snapshot;
      let held = previous?.source === source ? previous : null;
      if (held?.status === "ready" && held.title !== title) {
        const matches = held.items.flatMap((item, index) => (item.title === title ? [index] : []));
        if (matches.length === 1) {
          const following = [...held.items.slice(matches[0] + 1), ...(held.prefetch ?? [])];
          held = { ...held, title, items: reconcile(held.items, following), prefetch: following.slice(5) };
        }
      }
      // Preserve the mounted list through track changes. Known next songs move
      // immediately; the native refresh supplies the new last row in place.
      set({ loading: true, recovering: false, snapshot: held });
      const unavailable = () => {
        // Accessibility can lag just behind a skip. Keep a validated list
        // briefly while retrying, without flashing an empty panel each poll.
        const retain = held?.status === "ready" && Date.now() - lastGoodAt < 6000;
        set({
          loading: false,
          recovering: !!retain,
          snapshot: retain ? held : { source, title, status: "unavailable", items: [] },
        });
      };
      try {
        const snapshot = await load(source, title, open);
        if (request !== generation) return;
        if (!snapshot || snapshot.source !== source || snapshot.title !== title || snapshot.status === "unavailable") {
          unavailable();
          return;
        }
        if (snapshot.status === "ready") lastGoodAt = Date.now();
        set({
          loading: false,
          recovering: false,
          snapshot: {
            ...snapshot,
            items: snapshot.status === "ready" ? reconcile(get().snapshot?.items ?? [], snapshot.items) : [],
            prefetch: snapshot.status === "ready" ? snapshot.prefetch?.slice(0, 5) : [],
          },
        });
      } catch {
        if (request === generation) unavailable();
      }
    },
  }));
}

export const useQueue = createQueueStore((source, title, open) => bridge.music.queue(source, title, open));
