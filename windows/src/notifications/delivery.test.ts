// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useIsland } from "../island/store";
import { DEFAULT_SETTINGS } from "../island/settings";
import { clearPendingNotifications, queueNotification, updateNotificationDelivery } from "./delivery";
const item = (id: number) => ({
  id,
  app: "Discord",
  aumid: "discord",
  time: 100,
  title: `Sender ${id}`,
  body: "Hello",
});
beforeEach(() => {
  vi.useFakeTimers();
  clearPendingNotifications();
  useIsland.setState({
    expanded: false,
    notice: null,
    noticeExpanded: false,
    inside: false,
    pinned: false,
    capturePreview: null,
    settings: { ...DEFAULT_SETTINGS },
  });
});
afterEach(() => {
  clearPendingNotifications();
  vi.clearAllTimers();
  vi.useRealTimers();
});
it("waits for a session notice instead of dropping a message, then shows queued sources in order", () => {
  useIsland.getState().showNotice({ id: "session", kind: "sessions", title: "Done", priority: 2, duration: 6 });
  queueNotification(item(1));
  queueNotification(item(2));
  expect(useIsland.getState().notice?.id).toBe("session");
  vi.advanceTimersByTime(6000);
  expect(useIsland.getState().notice?.id).toBe("notification:1");
  vi.advanceTimersByTime(5000);
  expect(useIsland.getState().notice?.id).toBe("notification:2");
});
it("does not replay a cleared or locked inbox", () => {
  useIsland.setState({ expanded: true, selected: "calendar" });
  queueNotification(item(1));
  updateNotificationDelivery({ access: "allowed", items: [] });
  useIsland.getState().collapse();
  expect(useIsland.getState().notice).toBeNull();
});
it("expires waiting messages rather than replaying stale banners after a long interaction", () => {
  useIsland.setState({ expanded: true, selected: "calendar" });
  queueNotification(item(1));
  vi.advanceTimersByTime(31_000);
  useIsland.getState().collapse();
  expect(useIsland.getState().notice).toBeNull();
});
