// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it, vi } from "vitest";
import { geometryOf, useIsland } from "../island/store";
import { NoticeView } from "../island/views/Closed";
import { NotificationsPage } from "./NotificationsPage";
import { notificationNotice, notificationPresentation, notificationSource } from "./presentation";
import { useNotifications } from "./store";
import type { Item } from "./types";

// SSR reads Zustand's initial snapshot; render the current test inbox instead.
vi.mock("./store", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./store")>();
  const store = actual.useNotifications;
  return {
    ...actual,
    useNotifications: Object.assign(
      (select: (state: ReturnType<typeof store.getState>) => unknown) => select(store.getState()),
      store,
    ),
  };
});

const discord: Item = {
  id: 1,
  app: "Discord",
  aumid: "com.squirrel.Discord.Discord",
  time: 100,
  title: "Alicja (#general, Friends)",
  body: "Hej!\nDruga linia",
};

afterEach(() => useNotifications.setState({ access: "unavailable", items: [] }));

it("shows Discord and the sender without message text anywhere in the compact island", () => {
  const notice = notificationNotice(discord);
  expect(notice.title).toBe("Discord · Alicja (#general, Friends)");
  expect(notice.detail).toBeUndefined();
  expect(notificationPresentation(discord)).not.toHaveProperty("message");
  expect(notice.image).toBeTruthy();
  expect(notice.opens).toBe("notifications");
  const html = renderToStaticMarkup(
    <NoticeView notice={notice} geometry={geometryOf(useIsland.getState())} size={{ width: 500, height: 32 }} />,
  );
  expect(html).toContain("Discord · Alicja (#general, Friends)");
  expect(html).toContain("n-notice-image");
  expect(html).not.toContain("Hej!");
  expect(html).not.toContain("Druga linia");
});

it("uses the real app icon when Windows supplies one", () => {
  const icon = "data:image/png;base64,native";
  expect(notificationPresentation({ ...discord, icon }).icon).toBe(icon);
  expect(notificationNotice({ ...discord, icon }).imageFallback).toBeTruthy();
});

it.each([
  { app: "Messenger", aumid: "messenger" },
  { app: "", aumid: "FACEBOOK.317180B0BB486_8xx8rvfyw5nnt!App" },
])("recognizes Messenger identity and supplies its icon", (source) => {
  const item = { ...discord, ...source, title: "Marek" };
  expect(notificationPresentation(item)).toMatchObject({ app: "Messenger", sender: "Marek" });
  expect(notificationNotice(item).image).toBeTruthy();
});

it("does not reveal or guess a sender when the app hides notification details", () => {
  const notice = notificationNotice({ ...discord, title: "Discord", body: "New message" });
  expect(notice.title).toBe("Discord");
  expect(notice.detail).toBeUndefined();
  expect(notificationPresentation({ ...discord, title: "" }).sender).toBeUndefined();
});

it("keeps other app identities even when a message mentions Discord or Messenger", () => {
  const item = { ...discord, app: "Slack", aumid: "com.slack", title: "Discord", body: "Messenger" };
  expect(notificationNotice(item).title).toBe("Slack · Discord");
  expect(notificationSource(item).fallbackIcon).toBeUndefined();
  expect(notificationSource({ app: "Discord Tools", aumid: "org.example.discord" }).fallbackIcon).toBeUndefined();
});

it("shows only the source icon and sender on the notifications page", () => {
  useNotifications.setState({ access: "allowed", items: [discord] });
  const html = renderToStaticMarkup(<NotificationsPage />);
  expect(html).toContain("nt-icon");
  expect(html).toContain("Discord");
  expect(html).toContain("Alicja (#general, Friends)");
  expect(html).not.toContain("Hej!");
  expect(html).not.toContain("Druga linia");
});

it("removes a visible message when the native inbox is cleared on lock or disable", () => {
  useIsland.setState({ notice: notificationNotice(discord) });
  useNotifications.getState().receive({ access: "allowed", items: [] });
  expect(useIsland.getState().notice).toBeNull();
});
