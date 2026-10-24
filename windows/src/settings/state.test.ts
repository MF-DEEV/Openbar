// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Prefs, PrefsPatch } from "../shared/prefs";

// Each save waits until the test answers it, as the backend would.
const saves: { patch: PrefsPatch; resolve: (p: Prefs) => void; reject: (e: unknown) => void }[] = [];
vi.mock("../shared/prefs", async (original) => ({
  ...(await original<typeof import("../shared/prefs")>()),
  setPrefs: (patch: PrefsPatch) =>
    new Promise<Prefs>((resolve, reject) => {
      saves.push({ patch, resolve, reject });
    }),
}));

const { useSettings } = await import("./state");
const { resolve } = await import("../shared/prefs");

const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  saves.length = 0;
  useSettings.setState({ prefs: resolve(null), error: null });
});

describe("settings window state", () => {
  it("shows a change at once, then takes the saved value", async () => {
    useSettings.getState().island("outline", true);
    expect(useSettings.getState().prefs.island.outline).toBe(true);
    expect(saves[0]!.patch).toEqual({ island: { outline: true } });

    saves[0]!.resolve(resolve({ island: { outline: true, silhouette: "capsule" } }));
    await settle();
    expect(useSettings.getState().prefs.island.silhouette).toBe("capsule");
    expect(useSettings.getState().error).toBeNull();
  });

  it("puts the value back and says why when saving fails", async () => {
    useSettings.getState().sessions("buddies", false);
    expect(useSettings.getState().prefs.sessions.buddies).toBe(false);
    expect(saves[0]!.patch).toEqual({ sessions: { buddies: false } });

    saves[0]!.reject("couldn't save settings: denied");
    await settle();
    expect(useSettings.getState().prefs.sessions.buddies).toBe(true);
    expect(useSettings.getState().error).toBe("couldn't save settings: denied");

    // The next change clears the message.
    useSettings.getState().island("outline", true);
    expect(useSettings.getState().error).toBeNull();
  });

  it("leaves the other half of the preferences alone", () => {
    useSettings.getState().patch({ island: { display: "cursor" } });
    const { prefs } = useSettings.getState();
    expect(prefs.island.display).toBe("cursor");
    expect(prefs.sessions).toEqual(resolve(null).sessions);
  });
});
