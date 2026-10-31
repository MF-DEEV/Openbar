// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useBattery } from "../island/battery";
import { DEFAULT_SETTINGS, type IslandSettings } from "../island/settings";
import { geometryOf, presentation, useIsland } from "../island/store";
import { useMusic } from "../music/store";
import { visitDuration } from "./motion";
import {
  endMascotCountdown,
  greetMascot,
  mascotAtRest,
  mascotEnabled,
  mascotShows,
  reactMascot,
  refreshMascot,
  resetMascot,
  setMascotKeepAwake,
  setMascotLowPower,
  useMascot,
  watchMascotCountdown,
} from "./service";

const initialIsland = useIsland.getState();
const initialMusic = useMusic.getState();
const initialBattery = useBattery.getState();

function apply(patch: Partial<IslandSettings>) {
  useIsland.setState({ settings: { ...useIsland.getState().settings, ...patch } });
  refreshMascot();
}

/** Launch with the companion on, out of the island, and nothing playing. */
function launch(patch: Partial<IslandSettings> = {}) {
  apply({ mascotEnabled: true, mascotHidesWhenIdle: false, idleContent: "none", ...patch });
}

const advance = (seconds: number) => vi.advanceTimersByTime(seconds * 1000);

beforeEach(() => {
  vi.useFakeTimers();
  useIsland.setState({ ...initialIsland, settings: DEFAULT_SETTINGS }, true);
  useMusic.setState(initialMusic, true);
  useBattery.setState(initialBattery, true);
  resetMascot();
});

afterEach(() => {
  resetMascot();
  vi.useRealTimers();
});

describe("at rest", () => {
  it("is off until switched on, and off with the island", () => {
    expect(mascotEnabled(DEFAULT_SETTINGS)).toBe(false);
    expect(mascotEnabled({ ...DEFAULT_SETTINGS, mascotEnabled: true })).toBe(true);
    expect(mascotEnabled({ ...DEFAULT_SETTINGS, mascotEnabled: true, enabled: false })).toBe(false);
  });

  it("opens the closed island's wings for it, and folds them when it goes", () => {
    launch();
    const island = useIsland.getState();
    const geometry = geometryOf(island);
    expect(mascotAtRest(island)).toBe(true);
    expect(mascotShows(island, geometry)).toBe(true);
    const closed = presentation(island, geometry).size.width;
    const open = presentation(island, geometry, undefined, "none", true).size.width;
    expect(open).toBeGreaterThan(closed);
  });

  it("starts hidden at launch when it hides, as the Mac", () => {
    launch({ mascotHidesWhenIdle: true });
    expect(useMascot.getState().tucked).toBe(true);
    expect(mascotAtRest(useIsland.getState())).toBe(false);
  });

  it("gives way to a live activity and to idle content", () => {
    launch();
    useIsland.getState().setActivity("music", { wing: 30, opens: "music" });
    refreshMascot();
    const island = useIsland.getState();
    expect(mascotShows(island, geometryOf(island))).toBe(false);
    useIsland.getState().setActivity("music", null);
    apply({ idleContent: "music" });
    useMusic.setState({ playback: { isPlaying: true } as never });
    expect(mascotAtRest(useIsland.getState())).toBe(false);
  });

  it("wears an alert face while Keep Awake holds the PC up, and reacts only to changes", () => {
    launch();
    setMascotKeepAwake(false);
    expect(useMascot.getState().reaction).toBeNull();
    setMascotKeepAwake(true);
    expect(useMascot.getState().keepAwake).toBe(true);
  });
});

describe("switching on and off", () => {
  it("arrives when switched on, says goodbye when switched off, and folds after", () => {
    apply({ idleContent: "none", mascotHidesWhenIdle: false });
    apply({ mascotEnabled: true });
    expect(useMascot.getState().visit?.kind.type).toBe("arrive");
    advance(visitDuration({ type: "arrive" }) + 0.1);
    expect(useMascot.getState().visit).toBeNull();
    apply({ mascotEnabled: false });
    expect(useMascot.getState().visit?.kind.type).toBe("farewell");
    const island = useIsland.getState();
    // The farewell keeps it drawn, though it's off now.
    expect(mascotShows(island, geometryOf(island))).toBe(true);
    advance(visitDuration({ type: "farewell" }) + 0.1);
    expect(useMascot.getState().visit).toBeNull();
    const after = useIsland.getState();
    expect(mascotShows(after, geometryOf(after))).toBe(false);
  });

  it("doesn't take a launch with it on for being switched on", () => {
    apply({ mascotEnabled: true, mascotHidesWhenIdle: false, idleContent: "none" });
    expect(useMascot.getState().visit).toBeNull();
  });

  it("ends everything when the island is turned off", () => {
    launch();
    expect(greetMascot()).toBe(true);
    apply({ enabled: false });
    expect(useMascot.getState().visit).toBeNull();
    expect(useMascot.getState().on).toBe(false);
  });

  it("crosses behind the notch to a new side, and waits for a visit under way", () => {
    launch();
    apply({ mascotSide: "right" });
    expect(useMascot.getState().side).toBe("right");
    expect(useMascot.getState().visit?.kind.type).toBe("cross");
    advance(visitDuration({ type: "cross" }) + 0.2);
    expect(useMascot.getState().visit).toBeNull();
    greetMascot();
    apply({ mascotSide: "left" });
    expect(useMascot.getState().side).toBe("right");
    advance(visitDuration({ type: "lap" }) + 0.1);
    expect(useMascot.getState().side).toBe("left");
  });
});

describe("visits", () => {
  it("says hello soon after being turned on and keeps coming back", () => {
    const random = vi.spyOn(Math, "random").mockReturnValue(0.99);
    launch();
    advance(1.5);
    expect(useMascot.getState().visit?.kind.type).toBe("lap");
    expect(useMascot.getState().visit?.greeting).toBe("wink");
    advance(visitDuration({ type: "lap" }) + 0.1);
    expect(useMascot.getState().visit).toBeNull();
    advance(537);
    expect(useMascot.getState().visit?.kind.type).toBe("lap");
    random.mockRestore();
  });

  it("stays home with visits off, and Say Hi still comes", () => {
    launch({ mascotVisits: false });
    advance(2000);
    expect(useMascot.getState().visit).toBeNull();
    expect(greetMascot()).toBe(true);
    expect(useMascot.getState().visit?.kind.type).toBe("lap");
  });

  it("takes no strolls of its own in Energy saver, though asked it says hello", () => {
    launch();
    setMascotLowPower(true);
    advance(2000);
    expect(useMascot.getState().visit).toBeNull();
    expect(greetMascot()).toBe(true);
  });

  it("passes over what the island shows, stepping it aside", () => {
    launch();
    useIsland.getState().setActivity("music", { wing: 60, opens: "music" });
    refreshMascot();
    expect(greetMascot()).toBe(true);
    expect(useMascot.getState().visit?.kind.type).toBe("pass");
    expect(useMascot.getState().stepsAside).toBe(true);
  });

  it("answers Say Hi with no room: open, a notice, or switched off", () => {
    apply({ mascotHidesWhenIdle: false, idleContent: "none" });
    expect(greetMascot()).toBe(false);
    launch();
    useIsland.getState().open();
    expect(greetMascot()).toBe(false);
    useIsland.getState().collapse();
    useIsland.getState().showNotice({ id: "n", kind: "x", title: "t", priority: 1, duration: 3 });
    expect(greetMascot()).toBe(false);
  });

  it("doesn't start a second one over the first", () => {
    launch();
    expect(greetMascot()).toBe(true);
    const first = useMascot.getState().visit;
    expect(greetMascot()).toBe(false);
    expect(useMascot.getState().visit).toBe(first);
  });

  it("stays out of an island hidden until hover", () => {
    launch({ opening: "hiddenUntilHover" });
    expect(greetMascot()).toBe(false);
    useIsland.setState({ inside: true });
    expect(greetMascot()).toBe(true);
  });
});

describe("hiding when idle", () => {
  it("goes into the island a quiet half minute after it arrives, with a yawn", () => {
    apply({ idleContent: "none", mascotVisits: false });
    apply({ mascotEnabled: true });
    expect(useMascot.getState().tucked).toBe(false);
    // The quiet while counts from the moment it comes to rest.
    advance(visitDuration({ type: "arrive" }) + 30.1);
    expect(useMascot.getState().visit).toMatchObject({ kind: { type: "linger", reaction: "yawn" } });
    expect(useMascot.getState().tucked).toBe(true);
  });

  it("waits while the pointer is on it", () => {
    apply({ idleContent: "none", mascotVisits: false });
    apply({ mascotEnabled: true });
    useIsland.setState({ inside: true });
    advance(32);
    expect(useMascot.getState().tucked).toBe(false);
    useIsland.setState({ inside: false });
    advance(3.1);
    expect(useMascot.getState().tucked).toBe(true);
  });

  it("comes back out when hiding is turned off", () => {
    launch({ mascotHidesWhenIdle: true, mascotVisits: false });
    expect(useMascot.getState().tucked).toBe(true);
    apply({ mascotHidesWhenIdle: false });
    expect(useMascot.getState().tucked).toBe(false);
    expect(useMascot.getState().visit?.kind.type).toBe("arrive");
  });

  it("goes in soon when hiding is turned on, playing out what it's doing first", () => {
    launch({ mascotVisits: false });
    expect(greetMascot()).toBe(true);
    apply({ mascotHidesWhenIdle: true });
    expect(useMascot.getState().tucked).toBe(false);
    advance(visitDuration({ type: "lap" }) + 3.1);
    expect(useMascot.getState().tucked).toBe(true);
  });

  it("visits and reacts from inside the island all the same", () => {
    launch({ mascotHidesWhenIdle: true, mascotVisits: false });
    const island = useIsland.getState();
    expect(mascotShows(island, geometryOf(island))).toBe(false);
    reactMascot("celebrate");
    expect(useMascot.getState().visit).toMatchObject({ kind: { type: "cameo", reaction: "celebrate" } });
    const during = useIsland.getState();
    expect(mascotShows(during, geometryOf(during))).toBe(true);
  });
});

describe("reactions", () => {
  beforeEach(() => launch({ mascotVisits: false }));

  it("plays where it rests", () => {
    advance(0.3);
    reactMascot("celebrate");
    expect(useMascot.getState().reaction?.reaction).toBe("celebrate");
    expect(useMascot.getState().visit).toBeNull();
  });

  it("comes out over an activity instead", () => {
    useIsland.getState().setActivity("music", { wing: 60, opens: "music" });
    refreshMascot();
    advance(1);
    reactMascot("celebrate");
    expect(useMascot.getState().visit).toMatchObject({ kind: { type: "cameo", reaction: "celebrate" } });
    expect(useMascot.getState().stepsAside).toBe(true);
    advance(visitDuration({ type: "cameo", reaction: "celebrate" }) + 0.1);
    expect(useMascot.getState().visit).toBeNull();
    expect(useMascot.getState().stepsAside).toBe(false);
  });

  it("stays to react where it rested as an activity takes its place", () => {
    useIsland.getState().setActivity("music", { wing: 60, opens: "music" });
    refreshMascot();
    reactMascot("ready");
    expect(useMascot.getState().visit).toMatchObject({ kind: { type: "linger", reaction: "ready" } });
  });

  it("is off with its reactions off", () => {
    apply({ mascotReactions: false });
    advance(0.3);
    reactMascot("celebrate");
    expect(useMascot.getState().reaction).toBeNull();
    expect(useMascot.getState().visit).toBeNull();
  });

  it("spaces them out and doesn't repeat itself", () => {
    advance(0.3);
    reactMascot("celebrate");
    const first = useMascot.getState().reaction;
    advance(1);
    reactMascot("love");
    expect(useMascot.getState().reaction).toBe(first);
    advance(3);
    reactMascot("celebrate");
    expect(useMascot.getState().reaction).toBe(first);
    reactMascot("love");
    expect(useMascot.getState().reaction?.reaction).toBe("love");
  });

  it("waits for the island to show it, as long as its patience lasts", () => {
    advance(0.3);
    useIsland.getState().open();
    refreshMascot();
    reactMascot("celebrate", 8);
    advance(2);
    useIsland.getState().collapse();
    refreshMascot();
    advance(0.3);
    refreshMascot();
    expect(useMascot.getState().reaction?.reaction).toBe("celebrate");
  });

  it("drops one past its patience", () => {
    advance(0.3);
    useIsland.getState().open();
    refreshMascot();
    reactMascot("celebrate", 1);
    advance(2);
    useIsland.getState().collapse();
    refreshMascot();
    advance(0.3);
    refreshMascot();
    expect(useMascot.getState().reaction).toBeNull();
  });

  it("leaves a reaction to the notice that shows it", () => {
    advance(0.3);
    useIsland.getState().showNotice({
      id: "n",
      kind: "agents",
      title: "t",
      priority: 1,
      duration: 3,
      mascot: "celebrate",
    });
    reactMascot("celebrate");
    expect(useMascot.getState().reaction).toBeNull();
    useIsland.getState().dismissNotice();
    refreshMascot();
    advance(0.3);
    refreshMascot();
    expect(useMascot.getState().reaction).toBeNull();
  });
});

describe("countdowns", () => {
  it("watches beside the notch, retreats when paused, and is gone with the strip", () => {
    launch({ mascotVisits: false });
    useIsland.getState().setActivity("timer", { wing: 60, opens: "timer" });
    refreshMascot();
    watchMascotCountdown(5);
    expect(useMascot.getState().visit).toMatchObject({ kind: { type: "countdown", total: 5 } });
    endMascotCountdown(true);
    expect(useMascot.getState().visit?.kind.type).toBe("retreat");
    advance(0.4);
    expect(useMascot.getState().visit).toBeNull();
    watchMascotCountdown(5);
    useIsland.getState().setActivity("timer", null);
    refreshMascot();
    expect(useMascot.getState().visit?.kind.type).not.toBe("countdown");
  });

  it("doesn't come for the last second, nor in a capsule", () => {
    launch({ mascotVisits: false });
    useIsland.getState().setActivity("timer", { wing: 60, opens: "timer" });
    refreshMascot();
    watchMascotCountdown(1);
    expect(useMascot.getState().visit).toBeNull();
    apply({ silhouette: "capsule" });
    watchMascotCountdown(5);
    expect(useMascot.getState().visit).toBeNull();
  });
});
