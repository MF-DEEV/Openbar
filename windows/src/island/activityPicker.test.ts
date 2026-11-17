// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { beforeEach, describe, expect, it, vi } from "vitest";
import { activityPickerLayout, activitySize } from "./geometry";
import { compactActivities, geometryOf, presentation, useIsland } from "./store";

const initial = useIsland.getState();
beforeEach(() => useIsland.setState(initial, true));
const simultaneous = () => {
  const state = useIsland.getState();
  state.setActivity("sessions", { wing: 44, opens: "sessions" });
  state.setActivity("music", { wing: 56, capsuleContent: 56, opens: "music" });
  return useIsland.getState();
};

describe("compact activity picker", () => {
  it("opens the picker first when music and sessions coexist, then opens the chosen full page", () => {
    simultaneous();
    expect(useIsland.getState().activateActivity("sessions")).toBe(false);
    let state = useIsland.getState();
    expect(state.expanded).toBe(false);
    const before = presentation(state, geometryOf(state));
    expect(before.kind).toBe("picker");
    state.selectCompactActivity("music");
    state = useIsland.getState();
    expect(presentation(state, geometryOf(state)).size).toEqual(before.size);
    expect(state.pickedActivity).toBe("music");
    expect(state.activateActivity("music")).toBe(true);
    expect(useIsland.getState()).toMatchObject({ expanded: true, selected: "music", activityPickerOpen: false });
  });
  it("only subtly grows on hover and waits for a click to show choices", () => {
    vi.useFakeTimers();
    simultaneous();
    useIsland.setState((s) => ({ autoPresentAllowed: true, settings: { ...s.settings, opening: "hoverExpand" } }));
    useIsland.getState().pointerEnter();
    vi.advanceTimersByTime(1000);
    const state = useIsland.getState();
    expect(presentation(state, geometryOf(state)).kind).toBe("activity");
    expect(state.hoverEmphasized).toBe(true);
    expect(state.activityPickerOpen).toBe(false);
    expect(state.expanded).toBe(false);
    state.pointerExit();
    expect(presentation(useIsland.getState(), geometryOf(state)).kind).toBe("activity");
    vi.useRealTimers();
  });
  it("keeps ordinary opening with a single activity and ignores the microphone as a choice", () => {
    const state = useIsland.getState();
    state.setActivity("microphone", { wing: 54, opens: "sessions" });
    state.setActivity("sessions", { wing: 44, opens: "sessions" });
    expect(compactActivities(useIsland.getState())).toEqual(["sessions"]);
    expect(state.activateActivity("sessions")).toBe(true);
    expect(useIsland.getState().selected).toBe("sessions");
  });
  it("cancels automatic opening when a second activity arrives during the hover delay", () => {
    vi.useFakeTimers();
    useIsland.setState((s) => ({ autoPresentAllowed: true, settings: { ...s.settings, opening: "hoverExpand" } }));
    const state = useIsland.getState();
    state.setActivity("sessions", { wing: 44, opens: "sessions" });
    state.pointerEnter();
    state.setActivity("music", { wing: 56, opens: "music" });
    vi.advanceTimersByTime(1000);
    const current = useIsland.getState();
    expect(presentation(current, geometryOf(current)).kind).toBe("activity");
    expect(current.hoverEmphasized).toBe(true);
    expect(current.activityPickerOpen).toBe(false);
    expect(current.expanded).toBe(false);
    state.pointerExit();
    vi.useRealTimers();
  });
  it("hides when an activity ends and never retains a vanished selection", () => {
    simultaneous().selectCompactActivity("music");
    useIsland.getState().setActivity("music", null);
    const state = useIsland.getState();
    expect(state.pickedActivity).toBeNull();
    expect(state.activityPickerOpen).toBe(false);
    expect(presentation(state, geometryOf(state)).kind).toBe("activity");
  });
  it("omits disabled sections and stays dismissed until the pointer leaves", () => {
    simultaneous().activateActivity("sessions");
    useIsland.setState({ inside: true });
    useIsland.getState().dismissActivityPicker();
    let state = useIsland.getState();
    expect(presentation(state, geometryOf(state)).kind).toBe("activity");
    state.pointerExit();
    useIsland.setState((s) => ({ settings: { ...s.settings, hiddenModules: ["music"] } }));
    state = useIsland.getState();
    expect(compactActivities(state)).toEqual(["sessions"]);
  });
  it("keeps all rows inside both silhouettes, including the widest contour settings", () => {
    simultaneous();
    for (const silhouette of ["notch", "capsule"] as const)
      for (const roundness of [0, 100, 200])
        for (const shoulderWidth of [0, 100, 300]) {
          const state = useIsland.getState();
          const geometry = geometryOf({
            ...state,
            settings: { ...state.settings, silhouette, roundness, shoulderWidth },
          });
          const strips = compactActivities(state).map((id) =>
            activitySize(geometry, state.activities[id]!.wing, state.activities[id]!.capsuleContent),
          );
          const layout = activityPickerLayout(geometry, strips);
          expect(layout.size.width - 2 * layout.horizontal).toBeGreaterThanOrEqual(
            Math.max(...strips.map((s) => s.width)),
          );
          expect(layout.size.height - layout.headerHeight).toBeGreaterThanOrEqual(56);
          expect(layout.columns).toBe(2);
        }
  });
});
