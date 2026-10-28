import { beforeEach, describe, expect, it } from "vitest";
import { galleryHeight, geometryOf, presentation, transitionBetween, useIsland, visibleActivity } from "./store";

const initial = useIsland.getState();
beforeEach(() => useIsland.setState(initial, true));

describe("presentation", () => {
  it("rests at the notch size and opens a list at 560 × 328", () => {
    const s = useIsland.getState();
    expect(presentation(s, geometryOf(s)).size).toEqual({ width: 135, height: 24 });
    s.open("mixer");
    const open = useIsland.getState();
    expect(presentation(open, geometryOf(open))).toMatchObject({ kind: "expanded", size: { width: 560, height: 328 } });
  });

  it("shows three gallery rows at most: 560 × 338", () => {
    expect(galleryHeight(17, 560)).toBe(274);
    useIsland.getState().open(undefined, { sections: true });
    const s = useIsland.getState();
    expect(presentation(s, geometryOf(s)).size).toEqual({ width: 560, height: 338 });
  });

  it("pulses on hover when clicking opens", () => {
    useIsland.getState().pointerEnter();
    const s = useIsland.getState();
    expect(presentation(s, geometryOf(s)).size.width).toBeCloseTo(135 * 1.03);
    expect(presentation(s, geometryOf(s)).size.height).toBeCloseTo(24 * 1.03);
  });
});

describe("media activity priority", () => {
  it("keeps the session completion strip visible while media plays", () => {
    const s = useIsland.getState();
    s.setActivity("sessions", { wing: 30, opens: "sessions" });
    s.setActivity("music", { wing: 30, opens: "music" });
    expect(visibleActivity(useIsland.getState())).toBe("sessions");
    const state = useIsland.getState();
    expect(presentation(state, geometryOf(state)).key).toBe("activity:sessions");
    s.setActivity("music", null);
    expect(visibleActivity(useIsland.getState())).toBe("sessions");
  });

  it("keeps working or waiting sessions above playback", () => {
    const s = useIsland.getState();
    s.setActivity("sessions", { wing: 30, opens: "sessions" });
    s.setActivity("music", { wing: 30, opens: "music" });
    expect(visibleActivity(useIsland.getState())).toBe("sessions");
  });

  it("respects the manually picked activity", () => {
    useIsland.setState({
      activities: {
        sessions: { wing: 30, opens: "sessions" },
        music: { wing: 30, opens: "music" },
      },
      pickedActivity: "sessions",
    });
    expect(visibleActivity(useIsland.getState())).toBe("sessions");
  });

  it("keeps timers above playback even when sessions have completed", () => {
    const s = useIsland.getState();
    s.setActivity("sessions", { wing: 30, opens: "sessions" });
    s.setActivity("music", { wing: 30, opens: "music" });
    s.setActivity("timer", { wing: 30, opens: "timer" });
    expect(visibleActivity(useIsland.getState())).toBe("timer");
  });
});

describe("state machine", () => {
  it("opens Sessions by default on first click and after visiting another page", () => {
    const s = useIsland.getState();
    s.clickClosed();
    expect(useIsland.getState()).toMatchObject({ expanded: true, selected: "sessions", showingSections: false });
    s.select("controls");
    s.collapse();
    s.clickClosed();
    expect(useIsland.getState()).toMatchObject({ expanded: true, selected: "sessions", showingSections: false });
  });

  it("escape closes the gallery first, then the island", () => {
    const s = useIsland.getState();
    s.open(undefined, { sections: true });
    s.stepBack();
    expect(useIsland.getState()).toMatchObject({ expanded: true, showingSections: false });
    s.stepBack();
    expect(useIsland.getState().expanded).toBe(false);
  });

  it("a lower-priority notice cannot replace a higher one", () => {
    const s = useIsland.getState();
    s.showNotice({ id: "a", kind: "timer", title: "Timer", priority: 2, duration: 6 });
    s.showNotice({ id: "b", kind: "clipboard", title: "Copied", priority: 0, duration: 2.5 });
    expect(useIsland.getState().notice?.id).toBe("a");
    s.showNotice({ id: "c", kind: "volume", title: "Volume", priority: 3, duration: 1.6 });
    expect(useIsland.getState().notice?.id).toBe("c");
  });

  it("a tap on a notice opens its section, then runs what the notice adds", () => {
    const seen: unknown[] = [];
    useIsland.getState().showNotice({
      id: "n",
      kind: "sessions",
      title: "api",
      priority: 2,
      duration: 6,
      opens: "sessions",
      session: { id: "s1", provider: "claude", event: "finished" },
      onOpen: () => seen.push(useIsland.getState().selected),
    });
    useIsland.getState().clickClosed();
    expect(useIsland.getState()).toMatchObject({ notice: null, expanded: true, selected: "sessions" });
    expect(seen).toEqual(["sessions"]);
  });

  it("closing with the pointer inside suppresses hover until it leaves", () => {
    const s = useIsland.getState();
    s.pointerEnter();
    s.open();
    s.collapse();
    expect(useIsland.getState().hoverSuppressed).toBe(true);
    s.pointerExit();
    expect(useIsland.getState().hoverSuppressed).toBe(false);
  });
});

describe("content transitions (spec §3.2)", () => {
  const p = (kind: "resting" | "notice" | "peek" | "expanded", key: string) => ({
    kind,
    key,
    size: { width: 1, height: 1 },
    hasRow: false,
  });
  it("maps each change to the Mac's transition", () => {
    expect(transitionBetween(p("resting", "r"), p("expanded", "e:a"))).toBe("reveal");
    expect(transitionBetween(p("expanded", "e:a"), p("expanded", "e:b"))).toBe("replace");
    expect(transitionBetween(p("expanded", "e:a"), p("resting", "r"))).toBe("dismiss");
    expect(transitionBetween(p("notice", "n:a"), p("resting", "r"))).toBe("depart");
    expect(transitionBetween(p("resting", "r"), p("notice", "n:a"))).toBe("reveal");
    expect(transitionBetween(p("resting", "r"), p("resting", "r"))).toBe("none");
  });
});
