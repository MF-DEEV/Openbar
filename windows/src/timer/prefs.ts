// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The timer's preferences as the page edits them. A drag along the ruler
// changes the minutes many times a second; the page shows each value at
// once and writes the last one shortly after, so the round trip through
// prefs.rs never pulls the ruler back mid-gesture.

import { create } from "zustand";
import { useIsland } from "../island/store";
import type { IslandSettings } from "../island/settings";
import { setPrefs } from "../shared/prefs";

export type TimerPref =
  | "timerMode"
  | "timerMinutes"
  | "pomodoroFocusMinutes"
  | "pomodoroShortBreakMinutes"
  | "pomodoroLongBreakMinutes"
  | "pomodoroLongBreakInterval"
  | "pomodoroTotalSessions";

const WRITE_DELAY_MS = 150;

/** Values shown before their write comes back as settings. */
const useDrafts = create<Partial<Pick<IslandSettings, TimerPref>>>(() => ({}));
const writes = new Map<TimerPref, ReturnType<typeof setTimeout>>();

function clearDraft(key: TimerPref, value: unknown) {
  if (useDrafts.getState()[key] === value) useDrafts.setState({ [key]: undefined });
}

export function setTimerPref<K extends TimerPref>(key: K, value: IslandSettings[K]) {
  useDrafts.setState({ [key]: value });
  clearTimeout(writes.get(key));
  writes.set(
    key,
    setTimeout(() => {
      writes.delete(key);
      // The settings it returns are the ones the change event brings; taking
      // them now lets the draft go without showing the old value between.
      void setPrefs({ island: { [key]: value } })
        .then(
          (prefs) => useIsland.setState({ settings: prefs.island }),
          () => undefined,
        )
        .finally(() => clearDraft(key, value));
    }, WRITE_DELAY_MS),
  );
}

/** Runs `listener` whenever a pending value changes what the page shows. */
export const onTimerDrafts = (listener: () => void) => useDrafts.subscribe(listener);

/** The value to show: the draft while one is pending, else the stored setting. */
export function useTimerPref<K extends TimerPref>(key: K): IslandSettings[K] {
  const stored = useIsland((s) => s.settings[key]);
  const draft = useDrafts((s) => s[key]) as IslandSettings[K] | undefined;
  return draft ?? stored;
}

/** The same, outside React (Start reads what the page shows). */
export function timerPref<K extends TimerPref>(key: K): IslandSettings[K] {
  return (useDrafts.getState()[key] as IslandSettings[K] | undefined) ?? useIsland.getState().settings[key];
}
