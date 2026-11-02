// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's notification inbox (spec 04 §2): what the Rust listener
// mirrored, newest first, and whether Windows lets it listen at all.

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import { useIsland } from "../island/store";
import type { Access, Inbox, Item } from "./types";
import { updateNotificationDelivery } from "./delivery";

interface NotificationsState {
  access: Access;
  items: Item[];
  error: string | null;
  requesting: boolean;
  /** The Clear All question is up. */
  confirmingClear: boolean;
  receive(inbox: Inbox): void;
  requestAccess(): Promise<void>;
  dismiss(id: number): Promise<void>;
  clear(): Promise<void>;
}

export const useNotifications = create<NotificationsState>()((set) => ({
  access: "unavailable",
  items: [],
  error: null,
  requesting: false,
  confirmingClear: false,
  receive(inbox) {
    updateNotificationDelivery(inbox);
    set((s) => ({
      access: inbox.access,
      items: inbox.items,
      error: inbox.error ?? null,
      // Locking the PC empties the inbox under the question.
      confirmingClear: s.confirmingClear && inbox.items.length > 0,
    }));
    const island = useIsland.getState();
    if (
      island.notice?.kind === "systemNotification" &&
      (inbox.access !== "allowed" || !inbox.items.some((item) => `notification:${item.id}` === island.notice!.id))
    ) {
      island.dismissNotice(island.notice.id);
    }
  },
  async requestAccess() {
    set({ requesting: true });
    try {
      const access = await bridge.notifications.requestAccess();
      if (access) set({ access });
    } finally {
      set({ requesting: false });
    }
  },
  async dismiss(id) {
    const inbox = await bridge.notifications.dismiss(id);
    if (inbox) updateNotificationDelivery(inbox);
    if (inbox)
      set((s) => ({
        items: inbox.items,
        error: inbox.error ?? null,
        confirmingClear: s.confirmingClear && inbox.items.length > 0,
      }));
  },
  async clear() {
    set({ confirmingClear: false });
    const inbox = await bridge.notifications.clear();
    if (inbox) updateNotificationDelivery(inbox);
    if (inbox) set({ items: inbox.items, error: inbox.error ?? null });
  },
}));

export function subscribeNotifications(onNew: (item: Item) => void): () => void {
  let version = 0,
    live = true;
  const a = bridge.notifications.onInbox((i) => {
    version++;
    if (live) useNotifications.getState().receive(i);
  });
  void a.then(async () => {
    const at = version;
    const inbox = await bridge.notifications.inbox();
    if (live && inbox && at === version) useNotifications.getState().receive(inbox);
  });
  const b = bridge.notifications.onNew(onNew);
  return () => {
    live = false;
    void a.then((f) => f());
    void b.then((f) => f());
  };
}
