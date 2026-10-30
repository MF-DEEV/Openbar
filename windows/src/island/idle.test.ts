// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it } from "vitest";
import { resolve } from "../shared/prefs";
import type { BatteryReading } from "../system/types";
import { configuredIdleContent, visibleIdleContent } from "./idle";
import { DEFAULT_SETTINGS, type IdleContent, type IslandSettings } from "./settings";
import { geometryOf, presentation, useIsland } from "./store";

const settings = (patch: Partial<IslandSettings> = {}): IslandSettings => ({ ...DEFAULT_SETTINGS, ...patch });
const battery: BatteryReading = {
  percent: 64,
  charging: true,
  pluggedIn: true,
  rateWatts: null,
  secondsLeft: null,
  healthPercent: null,
  cycleCount: null,
};
const initial = useIsland.getState();
afterEach(() => useIsland.setState(initial, true));

describe("idle content preferences", () => {
  it("defaults to music, accepts the four Mac choices, and rejects invalid saved values", () => {
    expect(resolve(null).island.idleContent).toBe("music");
    for (const idleContent of ["none", "battery", "music", "agents"] as const) {
      expect(resolve({ island: { idleContent } }).island.idleContent).toBe(idleContent);
    }
    expect(resolve({ island: { idleContent: "all" as IdleContent } }).island.idleContent).toBe("music");
  });

  it("retains unavailable choices, restoring them when the feature returns", () => {
    const p = settings({ idleContent: "battery", hiddenSystemMetrics: ["power"] });
    expect(configuredIdleContent(p)).toBe("none");
    expect(p.idleContent).toBe("battery");
    expect(configuredIdleContent({ ...p, hiddenSystemMetrics: [] })).toBe("battery");
    for (const choice of ["music", "agents"] as const) {
      expect(configuredIdleContent(settings({ idleContent: choice, hiddenModules: [choice] }))).toBe("none");
      expect(configuredIdleContent(settings({ idleContent: choice, sections: { [choice]: false } }))).toBe("none");
      expect(configuredIdleContent(settings({ idleContent: choice }))).toBe(choice);
    }
  });

  it("requires real playback for music, and never substitutes another idle choice", () => {
    expect(visibleIdleContent(settings(), battery, false)).toBe("none");
    expect(visibleIdleContent(settings(), null, true)).toBe("music");
    expect(visibleIdleContent(settings({ showPlayingMusic: false }), battery, true)).toBe("none");
    expect(visibleIdleContent(settings({ idleContent: "none" }), battery, true)).toBe("none");
    expect(visibleIdleContent(settings({ idleContent: "battery" }), null, true)).toBe("none");
    expect(visibleIdleContent(settings({ idleContent: "battery" }), battery, false)).toBe("battery");
    expect(visibleIdleContent(settings({ idleContent: "agents" }), null, false)).toBe("agents");
  });
});

describe("idle island geometry and interactions", () => {
  it("uses the existing 44px wings and keeps the capsule's original dimensions", () => {
    for (const silhouette of ["notch", "capsule"] as const) {
      useIsland.setState({ settings: settings({ silhouette }) });
      const s = useIsland.getState();
      const g = geometryOf(s);
      const empty = presentation(s, g).size;
      const filled = presentation(s, g, undefined, "battery").size;
      expect(filled.height).toBe(empty.height);
      expect(filled.width).toBe(empty.width + (silhouette === "notch" ? 88 : 0));
    }
  });

  it("preserves notices, hover previews, live session status, opening and closing", () => {
    const show = () => {
      const s = useIsland.getState();
      return presentation(s, geometryOf(s), undefined, "battery");
    };
    expect(show().kind).toBe("resting");
    useIsland.getState().setActivity("sessions", { wing: 44, opens: "sessions", urgent: true });
    expect(show().key).toBe("activity:sessions");
    useIsland.getState().showNotice({ id: "battery-test", kind: "test", title: "Notice", priority: 2, duration: 3 });
    expect(show().kind).toBe("notice");
    useIsland.getState().dismissNotice();
    useIsland.getState().setActivity("sessions", null);
    useIsland.setState({ peeking: true });
    expect(show().kind).toBe("peek");
    useIsland.setState({ peeking: false });
    useIsland.getState().clickClosed();
    expect(show().kind).toBe("expanded");
    useIsland.getState().collapse();
    expect(show().kind).toBe("resting");
  });
});
