// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { geometryOf, presentation, transitionBetween, useIsland } from "../island/store";
import { DEFAULT_SETTINGS, type IslandSettings } from "../island/settings";
import { notificationNotice } from "./presentation";
import { useNotifications } from "./store";

const initial = useIsland.getState();
const item = (id: number, title = "Alicja") => ({
  id,
  app: "Discord",
  aumid: "com.squirrel.Discord.Discord",
  time: 100,
  title,
  body: "secret",
});
const island = () => useIsland.getState();
const configure = (settings: Partial<IslandSettings> = {}) =>
  useIsland.setState({
    settings: { ...DEFAULT_SETTINGS, opening: "hoverPreview", hoverDelay: 0.2, closeDelay: 0.3, ...settings },
  });

beforeEach(() => {
  vi.useFakeTimers();
  useIsland.setState(initial, true);
  configure();
});
afterEach(() => {
  island().dismissNotice();
  vi.useRealTimers();
});

describe("holding a notification banner", () => {
  it("opens under the pointer after the hover delay, then leaving dismisses it", () => {
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(199);
    expect(island().noticeExpanded).toBe(false);
    vi.advanceTimersByTime(1);
    expect(island().noticeExpanded).toBe(true);
    expect(presentation(island(), geometryOf(island()))).toMatchObject({
      kind: "noticePreview",
      key: "notice:preview:notification:1",
    });
    // Far past its own five seconds: the pointer holds it.
    vi.advanceTimersByTime(60_000);
    expect(island().notice?.id).toBe("notification:1");
    island().pointerExit();
    vi.advanceTimersByTime(299);
    expect(island().notice).not.toBeNull();
    vi.advanceTimersByTime(1);
    expect(island()).toMatchObject({ notice: null, noticeExpanded: false, expanded: false });
  });

  it("coming back before the close delay keeps the preview", () => {
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(200);
    island().pointerExit();
    vi.advanceTimersByTime(100);
    island().pointerEnter();
    vi.advanceTimersByTime(60_000);
    expect(island()).toMatchObject({ noticeExpanded: true, notice: { id: "notification:1" } });
  });

  it("a quick pass gives the banner its full time again", () => {
    island().showNotice(notificationNotice(item(1)));
    vi.advanceTimersByTime(4_500);
    island().pointerEnter();
    vi.advanceTimersByTime(100);
    island().pointerExit();
    vi.advanceTimersByTime(120 + 4_800);
    expect(island()).toMatchObject({ noticeExpanded: false, notice: { id: "notification:1" } });
    vi.advanceTimersByTime(200);
    expect(island().notice).toBeNull();
  });

  it("with opening on click the banner only waits", () => {
    configure({ opening: "click" });
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(60_000);
    expect(island()).toMatchObject({ noticeExpanded: false, notice: { id: "notification:1" } });
    island().pointerExit();
    vi.advanceTimersByTime(5_120);
    expect(island().notice).toBeNull();
  });

  it("hidden until hover opens the island rather than a preview", () => {
    configure({ opening: "hiddenUntilHover" });
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(200);
    expect(island()).toMatchObject({ noticeExpanded: false, expanded: true, notice: null });
  });

  it("a closed-with-pointer island holds the banner without opening it", () => {
    useIsland.setState({ hoverSuppressed: true });
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(60_000);
    expect(island()).toMatchObject({ noticeExpanded: false, notice: { id: "notification:1" } });
  });

  it("a message arriving under the pointer is held at once, and a newer one keeps the preview", () => {
    island().pointerEnter();
    vi.advanceTimersByTime(50);
    island().showNotice(notificationNotice(item(1)));
    vi.advanceTimersByTime(200);
    expect(island()).toMatchObject({ noticeExpanded: true, peeking: false, notice: { id: "notification:1" } });
    island().showNotice(notificationNotice(item(2, "Bartek")));
    expect(island()).toMatchObject({ noticeExpanded: true, notice: { id: "notification:2" } });
    vi.advanceTimersByTime(60_000);
    expect(island().notice?.id).toBe("notification:2");
  });

  it("a louder notice replaces the preview, and a quieter one waits", () => {
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(200);
    island().showNotice({ id: "copied", kind: "clipboard", title: "Copied", priority: 0, duration: 2 });
    expect(island().notice?.id).toBe("notification:1");
    island().showNotice({ id: "volume", kind: "volume", title: "Volume", priority: 3, duration: 2 });
    expect(island()).toMatchObject({ noticeExpanded: false, notice: { id: "volume" } });
    vi.advanceTimersByTime(2_000);
    expect(island().notice).toBeNull();
  });

  it("other notices are not held", () => {
    island().showNotice({ id: "timer", kind: "timer", title: "Timer", priority: 2, duration: 3 });
    island().pointerEnter();
    vi.advanceTimersByTime(3_000);
    expect(island()).toMatchObject({ notice: null, noticeExpanded: false });
  });

  it("a menu keeps the island but not a banner the pointer left", () => {
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(200);
    island().setHeld(true);
    island().pointerExit();
    expect(island()).toMatchObject({ notice: null, noticeExpanded: false, held: true });
  });

  it("collapsing or opening ends the preview, and a preview's control settles the hover", () => {
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(200);
    island().collapse();
    expect(island()).toMatchObject({ notice: null, noticeExpanded: false });

    useIsland.setState({ inside: false, hoverSuppressed: false });
    island().showNotice(notificationNotice(item(2)));
    island().pointerEnter();
    vi.advanceTimersByTime(200);
    island().settleNotice("other");
    expect(island().noticeExpanded).toBe(true);
    island().settleNotice("notification:2");
    expect(island()).toMatchObject({ notice: null, noticeExpanded: false, hoverSuppressed: true });
    vi.advanceTimersByTime(60_000);
    expect(island().expanded).toBe(false);
  });

  it("a notification gone from the inbox closes its preview", () => {
    useNotifications.setState({ items: [item(1)] });
    island().showNotice(notificationNotice(item(1)));
    island().pointerEnter();
    vi.advanceTimersByTime(200);
    useNotifications.getState().receive({ access: "allowed", items: [] });
    expect(island()).toMatchObject({ notice: null, noticeExpanded: false });
  });

  it("moves between banner and preview with the Mac's transitions", () => {
    const p = (kind: "notice" | "noticePreview" | "resting" | "expanded", key: string) => ({
      kind,
      key,
      size: { width: 1, height: 1 },
      hasRow: false,
    });
    expect(transitionBetween(p("notice", "a"), p("noticePreview", "b"))).toBe("reveal");
    expect(transitionBetween(p("noticePreview", "a"), p("noticePreview", "b"))).toBe("replace");
    expect(transitionBetween(p("noticePreview", "a"), p("notice", "b"))).toBe("replace");
    expect(transitionBetween(p("noticePreview", "a"), p("resting", "b"))).toBe("dismiss");
    expect(transitionBetween(p("noticePreview", "a"), p("expanded", "b"))).toBe("reveal");
  });
});

describe("the Clear All question", () => {
  afterEach(() => useNotifications.setState({ items: [], confirmingClear: false }));

  it("goes when the inbox empties, and stays while anything is left", () => {
    useNotifications.setState({ items: [item(1), item(2)], confirmingClear: true });
    useNotifications.getState().receive({ access: "allowed", items: [item(1)] });
    expect(useNotifications.getState().confirmingClear).toBe(true);
    useNotifications.getState().receive({ access: "allowed", items: [] });
    expect(useNotifications.getState().confirmingClear).toBe(false);
  });
});
