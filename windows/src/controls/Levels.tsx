// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The home page's levels (NotchControlsView.swift:142-331; spec 05 §1.1):
// volume with its mute and output chooser, and brightness, or the row that
// sets brightness up. Each is 28 tall; the slider shows the level it sets.

import { ChevronDown, Sun, Volume2, VolumeX } from "lucide-react";
import { useEffect } from "react";
import { modulesOf, useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { MenuButton, type MenuEntry } from "../shared/ui/Menu";
import { Slider } from "../shared/ui/Slider";
import { shownLevel, useAudio } from "../audio/store";
import { chosenDisplay, useBrightness } from "./brightness";
import { S } from "./strings";

/** The chevron alone: the slider beside it already shows the level (NotchLevelChooser). */
function LevelChooser(props: { label: string; entries: () => MenuEntry[] }) {
  return (
    <MenuButton className="ct-chooser" label={props.label} entries={props.entries}>
      <ChevronDown size={11} strokeWidth={2.5} aria-hidden="true" />
    </MenuButton>
  );
}

/** The outputs to choose, then the mixer where the island has it (NotchAudioControls.outputItems). */
export function outputEntries(): MenuEntry[] {
  const { state, setDefault } = useAudio.getState();
  const entries: MenuEntry[] = state.canSwitch
    ? state.outputs.map((d) => ({ label: d.name, checked: d.isDefault, onSelect: () => void setDefault(d.id) }))
    : [];
  if (modulesOf(useIsland.getState()).includes("mixer")) {
    if (entries.length > 0) entries.push({ kind: "separator" });
    entries.push({ label: S.mixer, icon: Volume2, onSelect: () => useIsland.getState().select("mixer") });
  }
  return entries;
}

export function VolumeLevel() {
  const output = useAudio((s) => s.state.output);
  const outputs = useAudio((s) => s.state.outputs);
  const canSwitch = useAudio((s) => s.state.canSwitch);
  const error = useAudio((s) => (s.switchError?.input === false ? s.switchError.message : null));
  const mixerShown = useIsland((s) => modulesOf(s).includes("mixer"));
  const level = shownLevel(output);
  const muted = output.muted === true;
  const device = outputs.find((d) => d.isDefault)?.name ?? S.output;
  return (
    <div className="ct-level" title={error ?? device}>
      <button
        type="button"
        className={`ct-level-glyph is-button${muted ? " is-muted" : ""}`}
        aria-label={muted ? S.unmute : S.mute}
        title={muted ? S.unmute : S.mute}
        disabled={output.muted === null}
        onClick={() => useAudio.getState().setOutput({ muted: !muted })}
      >
        {muted ? <VolumeX key="off" size={13} strokeWidth={2.25} /> : <Volume2 key="on" size={13} strokeWidth={2.25} />}
      </button>
      {level === null ? (
        <span className="ct-level-note">{S.outputUnavailable}</span>
      ) : (
        <Slider
          label={S.volume}
          value={level}
          valueText={`${Math.round(level * 100)}%`}
          onChange={(v) => useAudio.getState().setOutput({ volume: v })}
        />
      )}
      {(canSwitch && outputs.length > 0) || mixerShown ? (
        <LevelChooser label={S.output} entries={outputEntries} />
      ) : null}
    </div>
  );
}

export function BrightnessLevel() {
  const enabled = useIsland((s) => s.settings.brightnessControlEnabled);
  return enabled ? <BrightnessControls /> : <BrightnessSetup />;
}

/** A narrow level keeps the sun and the way to set brightness up; a wider one names it. */
function BrightnessSetup() {
  return (
    <div className="ct-level ct-setup" title={S.brightness}>
      <span className="ct-level-glyph" aria-hidden="true">
        <Sun size={13} strokeWidth={2.25} />
      </span>
      <span className="ct-setup-title">{S.brightness}</span>
      <button
        type="button"
        className="ct-setup-button"
        onClick={() => {
          useIsland.getState().collapse();
          void bridge.openSettings();
        }}
      >
        {S.settings}
      </button>
    </div>
  );
}

function BrightnessControls() {
  const displays = useBrightness((s) => s.displays);
  const picked = useBrightness((s) => s.picked);
  const loaded = useBrightness((s) => s.loaded);
  const display = chosenDisplay(displays, picked);
  useEffect(() => void useBrightness.getState().refresh(), []);
  return (
    <div className="ct-level" title={display?.name ?? S.noDisplays}>
      <span className="ct-level-glyph" aria-hidden="true">
        <Sun size={13} strokeWidth={2.25} />
      </span>
      {display ? (
        <Slider
          label={S.brightness}
          value={display.brightness}
          valueText={`${Math.round(display.brightness * 100)}%`}
          onChange={(v) => useBrightness.getState().set(display.id, v)}
        />
      ) : (
        <span className="ct-level-note">{loaded ? S.noDisplays : ""}</span>
      )}
      {display && displays.length > 1 ? (
        <LevelChooser
          label={S.brightness}
          entries={() =>
            displays.map((d) => ({
              label: d.name,
              checked: d.id === display.id,
              onSelect: () => useBrightness.getState().pick(d.id),
            }))
          }
        />
      ) : null}
    </div>
  );
}
