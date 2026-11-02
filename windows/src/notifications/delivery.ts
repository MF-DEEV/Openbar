// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { modulesOf, useIsland } from "../island/store";
import { notificationNotice } from "./presentation";
import type { Inbox, Item } from "./types";

let pending: { item: Item; received: number }[] = [];
let draining = false;
export function clearPendingNotifications() {
  pending = [];
}
export function updateNotificationDelivery(inbox: Inbox) {
  const ids = new Set(inbox.items.map((item) => item.id));
  pending = inbox.access === "allowed" ? pending.filter((entry) => ids.has(entry.item.id)) : [];
}
function drain() {
  if (draining || pending.length === 0) return;
  const island = useIsland.getState();
  if (!island.settings.enabled || !modulesOf(island).includes("notifications")) {
    pending = [];
    return;
  }
  pending = pending.filter((entry) => Date.now() - entry.received < 30_000);
  if (island.expanded && island.selected === "notifications" && !island.showingSections) {
    pending = [];
    return;
  }
  if (island.expanded || island.notice || island.capturePreview) return;
  const next = pending.shift();
  if (!next) return;
  draining = true;
  try {
    island.showNotice(notificationNotice(next.item));
  } finally {
    draining = false;
  }
}
export function queueNotification(item: Item) {
  if (
    useIsland.getState().notice?.id === `notification:${item.id}` ||
    pending.some((entry) => entry.item.id === item.id)
  )
    return;
  pending.push({ item, received: Date.now() });
  if (pending.length > 10) pending.shift();
  drain();
}
useIsland.subscribe((state, before) => {
  if (
    state.notice !== before.notice ||
    state.expanded !== before.expanded ||
    state.settings !== before.settings ||
    state.selected !== before.selected ||
    state.capturePreview !== before.capturePreview
  )
    drain();
});
