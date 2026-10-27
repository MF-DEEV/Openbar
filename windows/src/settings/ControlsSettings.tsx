// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Content › Controls and Mixer (NotchSettings.swift:380-400,
// 985-1006): the home page's levels and card, its shortcut tiles in order,
// brightness and the Keep Awake duration; the mixer's list options.

import { ArrowDown, ArrowUp } from "lucide-react";
import { editableShortcuts } from "../controls/layout";
import type { ControlItem } from "../controls/types";
import { useSettings } from "./state";
import { T } from "./strings";
import { Button, Select, SubRow, Toggle } from "./ui";

const C = T.controls;

/** The Mac's durations (Defaults.allowedDurations); 0 runs until turned off. */
const DURATIONS = [0, 15, 30, 60, 120, 240, 480];

const TITLES: Partial<Record<ControlItem, string>> = {
  music: C.music,
  volume: C.volume,
  brightness: C.brightness,
  keepAwake: C.keepAwake,
  microphone: C.microphone,
  mixer: C.mixer,
  timer: C.timer,
  calendar: C.calendar,
  screenshot: C.screenshot,
  speedTest: C.speedTest,
  scratchpad: C.scratchpad,
};

export function ControlsOptions() {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const hidden = island.hiddenControls;
  const shown = (item: ControlItem) => !hidden.includes(item);
  const setShown = (item: ControlItem, on: boolean) =>
    set("hiddenControls", on ? hidden.filter((i) => i !== item) : [...hidden, item]);
  const tiles = editableShortcuts(island.controlOrder);
  const move = (item: ControlItem, by: -1 | 1) => {
    const next = [...tiles];
    const from = next.indexOf(item);
    const to = from + by;
    if (to < 0 || to >= next.length) return;
    [next[from], next[to]] = [next[to]!, next[from]!];
    set("controlOrder", next);
  };

  return (
    <>
      {(["music", "volume", "brightness"] as const).map((item) => (
        <SubRow key={item} title={TITLES[item]!} caption={item === "volume" ? C.levelsCaption : undefined}>
          <Toggle label={TITLES[item]!} checked={shown(item)} onChange={(on) => setShown(item, on)} />
        </SubRow>
      ))}
      <SubRow title={C.brightnessSliders} caption={C.brightnessSlidersCaption} disabled={!shown("brightness")}>
        <Toggle
          label={C.brightnessSliders}
          checked={island.brightnessControlEnabled}
          disabled={!shown("brightness")}
          onChange={(on) => set("brightnessControlEnabled", on)}
        />
      </SubRow>
      <SubRow title={C.keepAwakeFor} caption={C.keepAwakeForCaption}>
        <Select<string>
          label={C.keepAwakeFor}
          value={String(DURATIONS.includes(island.keepAwakeDuration) ? island.keepAwakeDuration : 0)}
          options={DURATIONS.map((m) => ({ value: String(m), label: m === 0 ? C.untilOff : C.minutes(m) }))}
          onChange={(v) => set("keepAwakeDuration", Number(v))}
        />
      </SubRow>
      <SubRow title={C.shortcuts} caption={C.shortcutsCaption} />
      {tiles.map((item, index) => {
        const title = TITLES[item] ?? item;
        return (
          <SubRow key={item} title={title}>
            <button
              type="button"
              className="st-icon-button"
              aria-label={`${title}: ${T.content.moveUp}`}
              title={T.content.moveUp}
              disabled={index === 0}
              onClick={() => move(item, -1)}
            >
              <ArrowUp size={16} aria-hidden />
            </button>
            <button
              type="button"
              className="st-icon-button"
              aria-label={`${title}: ${T.content.moveDown}`}
              title={T.content.moveDown}
              disabled={index === tiles.length - 1}
              onClick={() => move(item, 1)}
            >
              <ArrowDown size={16} aria-hidden />
            </button>
            <Toggle label={title} checked={shown(item)} onChange={(on) => setShown(item, on)} />
          </SubRow>
        );
      })}
    </>
  );
}

export function MixerOptions() {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const patch = useSettings((s) => s.patch);
  const hiddenCount = Object.keys(island.mixerHiddenApps).length;
  return (
    <>
      <p className="st-note">{C.mixerCaption}</p>
      <SubRow title={C.hideInactive} caption={C.hideInactiveCaption}>
        <Toggle
          label={C.hideInactive}
          checked={island.mixerHideInactiveApps}
          onChange={(on) => set("mixerHideInactiveApps", on)}
        />
      </SubRow>
      <SubRow title={C.hiddenApps} caption={C.hiddenAppsCaption(hiddenCount)}>
        <Button disabled={hiddenCount === 0} onClick={() => patch({ island: { mixerHiddenApps: null } })}>
          {C.showAll}
        </Button>
      </SubRow>
    </>
  );
}
