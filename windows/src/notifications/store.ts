// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's notification inbox (spec 04 §2): what the Rust listener
// mirrored, newest first, and whether Windows lets it listen at all.

import { create } from "zustand";
import { bridge } from "../shared/bridge";
import type { Access, Inbox, Item } from "./types";

interface NotificationsState {
  access: Access;
  items: Item[];
  receive(inbox: Inbox): void;
  requestAccess(): Promise<void>;
  dismiss(id: number): Promise<void>;
  clear(): Promise<void>;
}

export const useNotifications = create<NotificationsState>()((set) => ({
  access: "unavailable",
  items: [],
  receive: (inbox) => set({ access: inbox.access, items: inbox.items }),
  async requestAccess() {
    const access = await bridge.notifications.requestAccess();
    if (access) set({ access });
  },
  async dismiss(id) {
    const inbox = await bridge.notifications.dismiss(id);
    if (inbox) set({ items: inbox.items });
  },
  async clear() {
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
