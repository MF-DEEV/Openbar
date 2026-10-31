// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's notification inbox (spec 04 §2): what the Rust listener
// mirrored, newest first, and whether Windows lets it listen at all.

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import { useIsland } from "../island/store";
import type { Access, Inbox, Item } from "./types";

interface NotificationsState {
  access: Access;
  items: Item[];
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
  confirmingClear: false,
  receive(inbox) {
    set((s) => ({
      access: inbox.access,
      items: inbox.items,
      // Locking the PC empties the inbox under the question.
      confirmingClear: s.confirmingClear && inbox.items.length > 0,
    }));
    const island = useIsland.getState();
    if (
      island.notice?.kind === "systemNotification" &&
      !inbox.items.some((item) => `notification:${item.id}` === island.notice!.id)
    ) {
      island.dismissNotice(island.notice.id);
    }
  },
  async requestAccess() {
    const access = await bridge.notifications.requestAccess();
    if (access) set({ access });
  },
  async dismiss(id) {
    const inbox = await bridge.notifications.dismiss(id);
    if (inbox) set((s) => ({ items: inbox.items, confirmingClear: s.confirmingClear && inbox.items.length > 0 }));
  },
  async clear() {
    set({ confirmingClear: false });
    const inbox = await bridge.notifications.clear();
    if (inbox) set({ items: inbox.items });
  },
}));

export function subscribeNotifications(onNew: (item: Item) => void): () => void {
  void bridge.notifications.inbox().then((i) => i && useNotifications.getState().receive(i));
  const a = bridge.notifications.onInbox((i) => useNotifications.getState().receive(i));
  const b = bridge.notifications.onNew(onNew);
  return () => {
    void a.then((f) => f());
    void b.then((f) => f());
  };
}
