// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Content › Controls and Mixer (NotchSettings.swift:380-400,
// 985-1006): the home page's levels and card, its shortcut tiles in order,
// brightness and the Keep Awake duration; the mixer's options, with the
// Mac's Output switcher and Audio priority cards (GeneralToolSettings.swift).

import { ArrowDown, ArrowUp } from "lucide-react";
import { Fragment, useEffect } from "react";
import { latest, subscribeAudio, useAudio } from "../audio/store";
import { editableShortcuts } from "../controls/layout";
import type { ControlItem } from "../controls/types";
import { visibleModules } from "../island/modules";
import { useSettings } from "./state";
import { T } from "./strings";
import { DEFAULT_SWITCHER_SHORTCUT, sectionEnabled } from "../island/settings";
import { cycleOrder } from "../mixer/arrangement";
import { useSwitcherShortcut } from "../mixer/switcherShortcut";
import { S as M } from "../mixer/strings";
import { bridge } from "../shared/bridge";
import { ShortcutField } from "../shared/ui/ShortcutField";
import { Button, Select, Slider, SubRow, Toggle } from "./ui";

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
  // Every section, whatever the Content list shows: a hidden one still gets its tile back when shown.
  const tiles = editableShortcuts(
    island.controlOrder,
    visibleModules(island.moduleOrder, island.hiddenModules, (id) => sectionEnabled(island, id)),
  );
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
  useAudioState();
  const canSwitch = useAudio((s) => s.state.canSwitch);
  const rollerFailed = useAudio((s) => s.state.rollerFailed);
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
      <SubRow title={M.lowerOnHeadphonesDisconnect} caption={M.lowerOnHeadphonesDisconnectCaption}>
        <Toggle
          label={M.lowerOnHeadphonesDisconnect}
          checked={island.mixerLowerVolumeOnHeadphonesDisconnect}
          onChange={(on) => set("mixerLowerVolumeOnHeadphonesDisconnect", on)}
        />
      </SubRow>
      {island.mixerLowerVolumeOnHeadphonesDisconnect ? (
        <SubRow title={M.headphonesDisconnectVolume}>
          <Slider
            label={M.headphonesDisconnectVolume}
            min={0}
            max={100}
            step={5}
            value={island.mixerHeadphonesDisconnectVolumePercent}
            format={(v) => `${v}%`}
            onChange={(v) => set("mixerHeadphonesDisconnectVolumePercent", v)}
          />
        </SubRow>
      ) : null}
      <SubRow
        title={M.preciseRoller}
        caption={lines([
          M.preciseRollerCaption,
          island.preciseVolumeRollerEnabled && rollerFailed && M.preciseRollerFailed,
        ])}
      >
        <Toggle
          label={M.preciseRoller}
          checked={island.preciseVolumeRollerEnabled}
          onChange={(on) => set("preciseVolumeRollerEnabled", on)}
        />
      </SubRow>
      <SubRow title={C.hiddenApps} caption={C.hiddenAppsCaption(hiddenCount)}>
        <Button disabled={hiddenCount === 0} onClick={() => patch({ island: { mixerHiddenApps: null } })}>
          {C.showAll}
        </Button>
      </SubRow>
      {canSwitch ? (
        <>
          <OutputSwitcherOptions />
          <AudioPriorityOptions />
        </>
      ) : null}
    </>
  );
}

/** The settings window doesn't show the island, so it listens to the audio state itself. */
let listening = false;
function useAudioState() {
  useEffect(() => {
    if (listening) return;
    listening = true;
    void subscribeAudio();
  }, []);
}

/** SoundOutputSwitcherControls, as Settings › Output switcher draws it on the Mac. */
function OutputSwitcherOptions() {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const patch = useSettings((s) => s.patch);
  const outputs = useAudio((s) => s.state.outputs);
  const refused = useAudio((s) => s.state.switcherRefused);
  const failed = useAudio((s) => s.state.switchFailed);
  const on = island.soundOutputSwitcherEnabled;
  const chosen = island.soundOutputSwitcherDevices;
  const enable = (v: boolean) => {
    const current = outputs.find((d) => d.isDefault);
    if (v && chosen.length === 0 && current)
      patch({ island: { soundOutputSwitcherEnabled: true, soundOutputSwitcherDevices: [current.id] } });
    else set("soundOutputSwitcherEnabled", v);
  };
  const choose = (id: string, v: boolean) =>
    set(
      "soundOutputSwitcherDevices",
      cycleOrder(v ? [...chosen.filter((c) => c !== id), id] : chosen.filter((c) => c !== id), outputs),
    );
  const row = useSwitcherShortcut((accelerator) => set("soundOutputSwitcherShortcut", accelerator), island.shortcut);
  const shortcut = island.soundOutputSwitcherShortcut;
  return (
    <>
      <SubRow title={M.switcherTitle} />
      <SubRow title={M.switcher} caption={M.switcherCaption}>
        <Toggle label={M.switcher} checked={on} onChange={enable} />
      </SubRow>
      {on ? (
        <>
          <SubRow
            title={M.shortcut}
            caption={lines([
              row.problem ?? (row.recording && M.shortcutRecordingCaption),
              refused && !row.recording && M.shortcutUnavailable,
              failed && M.switcherNoAvailable,
            ])}
          >
            <ShortcutField
              className="st-button st-shortcut"
              label={M.shortcut}
              recordingLabel={M.shortcutPressKeys}
              value={shortcut}
              {...row.field}
            />
            <Button
              disabled={shortcut === DEFAULT_SWITCHER_SHORTCUT}
              onClick={() => row.reset(DEFAULT_SWITCHER_SHORTCUT)}
            >
              {M.shortcutReset}
            </Button>
          </SubRow>
          <SubRow title={M.switcherDevices} caption={outputs.length === 0 ? M.noOutputs : undefined} />
          {outputs.map((d) => (
            <SubRow key={d.id} title={d.isDefault ? `${d.name} (${M.current})` : d.name}>
              <Toggle label={d.name} checked={chosen.includes(d.id)} onChange={(v) => choose(d.id, v)} />
            </SubRow>
          ))}
        </>
      ) : null}
    </>
  );
}

/** Several caption lines, the ones that apply; none leaves the row without a caption. */
function lines(all: (string | false | null | undefined)[]) {
  const shown = all.filter((l): l is string => typeof l === "string" && l.length > 0);
  if (shown.length === 0) return undefined;
  return (
    <>
      {shown.map((line, i) => (
        <span key={i} className="st-caption-line">
          {line}
        </span>
      ))}
    </>
  );
}

/** AudioPriorityDisclosure as Settings › Audio priority draws it: both lists, open. */
function AudioPriorityOptions() {
  const priority = useAudio((s) => s.state.priority);
  const outputError = useAudio((s) => s.state.priorityError);
  const inputError = useAudio((s) => s.state.inputPriorityError);
  const outputs = useAudio((s) => s.state.outputs);
  const inputs = useAudio((s) => s.state.inputs);
  const lists = [
    {
      output: true,
      title: M.priorityOutputs,
      help: M.priorityOutputEnable,
      enabled: priority.outputEnabled,
      devices: priority.outputs,
      error: outputError,
      current: outputs.find((d) => d.isDefault)?.id,
    },
    {
      output: false,
      title: M.priorityInputs,
      help: M.priorityInputEnable,
      enabled: priority.inputEnabled,
      devices: priority.inputs,
      error: inputError,
      current: inputs.find((d) => d.isDefault)?.id,
    },
  ];
  const move = (output: boolean, ids: string[], index: number, by: -1 | 1) => {
    const next = [...ids];
    [next[index], next[index + by]] = [next[index + by]!, next[index]!];
    latest(output ? "priority:out" : "priority:in", () => bridge.audio.setPriority(output, { ids: next }));
  };
  return (
    <>
      <SubRow title={M.priority} caption={M.priorityCaption} />
      {lists.map((list) => {
        const ids = list.devices.map((d) => d.id);
        return (
          <Fragment key={String(list.output)}>
            <SubRow
              title={list.title}
              caption={
                list.devices.length === 0
                  ? list.output
                    ? M.noOutputs
                    : M.noMicrophones
                  : list.error
                    ? M.couldNotSwitch(list.error)
                    : undefined
              }
            >
              <Toggle
                label={list.help}
                checked={list.enabled}
                onChange={(on) => void bridge.audio.setPriority(list.output, { enabled: on })}
              />
            </SubRow>
            {list.devices.map((d, index) => (
              <SubRow
                key={d.id}
                title={`${index + 1}. ${d.name}`}
                caption={d.id === list.current ? M.priorityCurrent : d.available ? undefined : M.priorityUnavailable}
              >
                <button
                  type="button"
                  className="st-icon-button"
                  aria-label={`${d.name}: ${M.priorityMoveUp}`}
                  title={M.priorityMoveUp}
                  disabled={index === 0}
                  onClick={() => move(list.output, ids, index, -1)}
                >
                  <ArrowUp size={16} aria-hidden />
                </button>
                <button
                  type="button"
                  className="st-icon-button"
                  aria-label={`${d.name}: ${M.priorityMoveDown}`}
                  title={M.priorityMoveDown}
                  disabled={index === ids.length - 1}
                  onClick={() => move(list.output, ids, index, 1)}
                >
                  <ArrowDown size={16} aria-hidden />
                </button>
              </SubRow>
            ))}
          </Fragment>
        );
      })}
    </>
  );
}
