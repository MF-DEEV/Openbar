// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The switches under the mixer's devices, as the Mac's island draws them
// (MixerOptionsControls in MixerSection.swift, SoundOutputSwitcherControls,
// AudioPriorityDisclosure in AudioPrioritySection.swift): lower the volume
// when headphones disconnect, finer volume steps, the output switcher and
// its shortcut, and the audio priority lists. The backend acts on all of
// them (audio/guard.rs, roller.rs, switcher.rs, priority.rs).

import { useLocale, tf } from "../i18n";

import {
  AlertTriangle,
  ChevronRight,
  Keyboard,
  ListOrdered,
  Menu as Lines,
  Mic,
  Minus,
  Plus,
  Speaker,
  Volume2,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { DEFAULT_SWITCHER_SHORTCUT } from "../island/settings";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import { Checkbox, Switch } from "../shared/ui/controls";
import { useReorder } from "../shared/ui/reorder";
import { ShortcutField } from "../shared/ui/ShortcutField";
import { latest, useAudio } from "../audio/store";
import type { RankedDevice } from "../audio/types";
import { cycleOrder } from "./arrangement";
import { useSwitcherShortcut } from "./switcherShortcut";
import { S } from "./strings";

/** Defaults.minimumMixerHeadphonesDisconnectVolumePercent…100, in steps of 5. */
const STEP = 5;

type Island = ReturnType<typeof useIsland.getState>["settings"];
const save = (patch: Partial<Island>) => void setPrefs({ island: patch });

function Message(props: { icon: ReactNode; children: ReactNode }) {
  useLocale();
  return (
    <p className="mx-option-message">
      <span aria-hidden="true">{props.icon}</span>
      {props.children}
    </p>
  );
}

function Caption({ children }: { children: ReactNode }) {
  useLocale();
  return <p className="mx-option-caption">{children}</p>;
}

export function HeadphoneGuard() {
  useLocale();
  const on = useIsland((s) => s.settings.mixerLowerVolumeOnHeadphonesDisconnect);
  const level = useIsland((s) => s.settings.mixerHeadphonesDisconnectVolumePercent);
  const set = (v: number) => save({ mixerHeadphonesDisconnectVolumePercent: Math.min(100, Math.max(0, v)) });
  return (
    <div className="mx-option-group">
      <Checkbox
        label={S.lowerOnHeadphonesDisconnect}
        checked={on}
        onChange={(v) => save({ mixerLowerVolumeOnHeadphonesDisconnect: v })}
      />
      <Caption>{S.lowerOnHeadphonesDisconnectCaption}</Caption>
      {on ? (
        <div className="mx-stepper-row">
          <span className="mx-stepper-title">{S.headphonesDisconnectVolume}</span>
          <span className="mx-stepper" role="group" aria-label={S.headphonesDisconnectVolume}>
            <button type="button" aria-label={S.lower} disabled={level <= 0} onClick={() => set(level - STEP)}>
              <Minus size={10} strokeWidth={2.75} />
            </button>
            <button type="button" aria-label={S.raise} disabled={level >= 100} onClick={() => set(level + STEP)}>
              <Plus size={10} strokeWidth={2.75} />
            </button>
          </span>
          <span className="mx-stepper-value" aria-live="polite">
            {level}%
          </span>
        </div>
      ) : null}
    </div>
  );
}

export function PreciseRoller() {
  useLocale();
  const on = useIsland((s) => s.settings.preciseVolumeRollerEnabled);
  const failed = useAudio((s) => s.state.rollerFailed);
  return (
    <div className="mx-option-group">
      <Checkbox label={S.preciseRoller} checked={on} onChange={(v) => save({ preciseVolumeRollerEnabled: v })} />
      <Caption>{S.preciseRollerCaption}</Caption>
      {on && failed ? <Message icon={<AlertTriangle size={9} />}>{S.preciseRollerFailed}</Message> : null}
    </div>
  );
}

export function OutputSwitcher() {
  useLocale();
  const on = useIsland((s) => s.settings.soundOutputSwitcherEnabled);
  const shortcut = useIsland((s) => s.settings.soundOutputSwitcherShortcut);
  const chosen = useIsland((s) => s.settings.soundOutputSwitcherDevices);
  const outputs = useAudio((s) => s.state.outputs);
  const refused = useAudio((s) => s.state.switcherRefused);
  const failed = useAudio((s) => s.state.switchFailed);
  const islandShortcut = useIsland((s) => s.settings.shortcut);
  const row = useSwitcherShortcut((accelerator) => save({ soundOutputSwitcherShortcut: accelerator }), islandShortcut);
  const enable = (v: boolean) => {
    const current = outputs.find((d) => d.isDefault);
    // Turned on with nothing chosen: the output playing now starts the cycle.
    if (v && chosen.length === 0 && current)
      save({ soundOutputSwitcherEnabled: true, soundOutputSwitcherDevices: [current.id] });
    else save({ soundOutputSwitcherEnabled: v });
  };
  const choose = (id: string, v: boolean) => {
    const next = v ? [...chosen.filter((c) => c !== id), id] : chosen.filter((c) => c !== id);
    save({ soundOutputSwitcherDevices: cycleOrder(next, outputs) });
  };
  return (
    <div className="mx-option-group">
      <Checkbox label={S.switcher} checked={on} onChange={enable} />
      <Caption>{S.switcherCaption}</Caption>
      {on ? (
        <>
          <div className="mx-option-row">
            <span className="mx-option-icon" aria-hidden="true">
              <Keyboard size={11} strokeWidth={2.25} />
            </span>
            <span className="mx-option-title">{S.shortcut}</span>
            <ShortcutField
              className="mx-shortcut"
              label={S.shortcut}
              recordingLabel={S.shortcutPressKeys}
              value={shortcut}
              {...row.field}
            />
            <button
              type="button"
              className="mx-reset-shortcut"
              disabled={shortcut === DEFAULT_SWITCHER_SHORTCUT}
              onClick={() => row.reset(DEFAULT_SWITCHER_SHORTCUT)}
            >
              {S.shortcutReset}
            </button>
          </div>
          {row.problem ? (
            <Message icon={<Keyboard size={9} />}>{row.problem}</Message>
          ) : row.recording ? (
            <Caption>{S.shortcutRecordingCaption}</Caption>
          ) : null}
          {refused && !row.recording ? <Message icon={<Keyboard size={9} />}>{S.shortcutUnavailable}</Message> : null}
          {failed ? <Message icon={<Speaker size={9} />}>{S.switcherNoAvailable}</Message> : null}
          <span className="mx-option-subtitle">{S.switcherDevices}</span>
          {outputs.length === 0 ? (
            <Message icon={<Speaker size={9} />}>{S.noOutputs}</Message>
          ) : (
            outputs.map((d) => (
              <Checkbox
                key={d.id}
                className="is-small"
                label={d.isDefault ? `${d.name} (${S.current})` : d.name}
                checked={chosen.includes(d.id)}
                onChange={(v) => choose(d.id, v)}
              />
            ))
          )}
        </>
      ) : null}
    </div>
  );
}

/** Ids in the newest order the person gave, until the backend's list agrees. */
function usePendingOrder(server: string[]) {
  const [local, setLocal] = useState<string[] | null>(null);
  const key = server.join("\u0000");
  useEffect(() => {
    setLocal((l) => (l && l.join("\u0000") === key ? null : l));
  }, [key]);
  return [local ?? server, setLocal] as const;
}

function PriorityList(props: { output: boolean; devices: RankedDevice[]; currentId: string | undefined }) {
  useLocale();
  const serverIds = props.devices.map((d) => d.id);
  const [ids, setLocal] = usePendingOrder(serverIds);
  const send = (next: string[]) => {
    setLocal(next);
    // A save that fails puts the backend's order back.
    latest(props.output ? "priority:out" : "priority:in", () =>
      bridge.audio.setPriority(props.output, { ids: next }).catch((e: unknown) => {
        setLocal(null);
        throw e;
      }),
    );
  };
  const reorder = useReorder({ ids, axis: "y", onReorder: send });
  const byId = new Map(props.devices.map((d) => [d.id, d]));
  const move = (id: string, by: -1 | 1) => {
    const from = ids.indexOf(id);
    const to = from + by;
    if (from < 0 || to < 0 || to >= ids.length) return;
    const next = [...ids];
    [next[from], next[to]] = [next[to]!, next[from]!];
    send(next);
  };
  if (ids.length === 0) return <p className="mx-option-caption">{props.output ? S.noOutputs : S.noMicrophones}</p>;
  return (
    <div className="mx-priority-list">
      {ids.map((id, index) => {
        const device = byId.get(id);
        const current = id === props.currentId;
        const name = device?.name ?? id;
        return (
          <div
            key={id}
            className={`mx-priority-row${current ? " is-current" : ""}${reorder.moving === id ? " is-moving" : ""}`}
            tabIndex={0}
            aria-label={`${index + 1}. ${name}${current ? `, ${S.priorityCurrent}` : ""}${device?.available === false ? `, ${S.priorityUnavailable}` : ""}`}
            title={tf("windows.mixer.valueValueAltAlt", S.priorityMoveUp, S.priorityMoveDown)}
            onKeyDown={(e) => {
              if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
              e.preventDefault();
              move(id, e.key === "ArrowUp" ? -1 : 1);
            }}
            {...reorder.itemProps(id)}
          >
            <span className="mx-priority-index">{index + 1}</span>
            <span className="mx-priority-text">
              <span className="mx-priority-line">
                <span className={`mx-priority-name${device?.available === false ? " is-off" : ""}`}>{name}</span>
                {current ? <span className="mx-priority-current">{S.priorityCurrent}</span> : null}
              </span>
              {device?.available === false ? (
                <span className="mx-priority-unavailable">{S.priorityUnavailable}</span>
              ) : null}
            </span>
            <Lines className="mx-priority-handle" size={11} strokeWidth={2.5} aria-hidden="true" />
          </div>
        );
      })}
    </div>
  );
}

function PrioritySubsection(props: { output: boolean }) {
  useLocale();
  const priority = useAudio((s) => s.state.priority);
  const devices = useAudio((s) => (props.output ? s.state.outputs : s.state.inputs));
  const enabled = props.output ? priority.outputEnabled : priority.inputEnabled;
  const help = props.output ? S.priorityOutputEnable : S.priorityInputEnable;
  const Icon = props.output ? Volume2 : Mic;
  return (
    <div className="mx-option-group">
      <div className="mx-priority-header">
        <Icon size={11} strokeWidth={2.25} aria-hidden="true" />
        <span className="mx-priority-title">{props.output ? S.priorityOutputs : S.priorityInputs}</span>
        <Switch
          label={help}
          on={enabled}
          onChange={(on) => void bridge.audio.setPriority(props.output, { enabled: on })}
        />
      </div>
      <PriorityList
        output={props.output}
        devices={props.output ? priority.outputs : priority.inputs}
        currentId={devices.find((d) => d.isDefault)?.id}
      />
    </div>
  );
}

export function AudioPriority(props: { initiallyExpanded?: boolean; showsHeader?: boolean }) {
  useLocale();
  const [expanded, setExpanded] = useState(props.initiallyExpanded ?? false);
  const header = props.showsHeader ?? true;
  return (
    <div className="mx-option-group">
      {header ? (
        <button type="button" className="mx-disclosure" aria-expanded={expanded} onClick={() => setExpanded((e) => !e)}>
          <ListOrdered size={11} strokeWidth={2.25} aria-hidden="true" />
          <span>{S.priority}</span>
          <ChevronRight className="mx-disclosure-chevron" size={10} strokeWidth={3} aria-hidden="true" />
        </button>
      ) : null}
      {expanded || !header ? (
        <div className={`mx-priority${header ? " is-indented" : ""}`}>
          <Caption>{S.priorityCaption}</Caption>
          <PrioritySubsection output />
          <div className="mx-options-divider" />
          <PrioritySubsection output={false} />
        </div>
      ) : null}
    </div>
  );
}
