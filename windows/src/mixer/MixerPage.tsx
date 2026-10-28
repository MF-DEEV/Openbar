// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The mixer as a desk (NotchMixerView.swift; spec 05 §2): the output and
// microphone faders, then one fader per app running sideways, pinned ones
// first. The band holds the output device and the options; the options
// layer replaces the desk and Escape closes it first. Windows' per-app
// volume stops at 100% and an app can't be sent to a device of its own, so
// there is no boost range, no Output ▸ menu and no route badge.

import {
  ArrowLeft,
  ArrowRight,
  ChevronDown,
  Ellipsis,
  EyeOff,
  Mic,
  MicOff,
  Pin,
  PinOff,
  RotateCcw,
  SlidersHorizontal,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import { type CSSProperties, useEffect, useState } from "react";
import { create } from "zustand";
import { usePageLayer } from "../island/pageLayer";
import { useIsland } from "../island/store";
import { EmptyState, IconButton } from "../island/views/Controls";
import type { PageProps } from "../island/views/Expanded";
import { bridge } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import { EditablePercent, Switch } from "../shared/ui/controls";
import { MenuButton, type MenuEntry, useMenu } from "../shared/ui/Menu";
import { useReorder } from "../shared/ui/reorder";
import { Slider } from "../shared/ui/Slider";
import { latest, percent, shownLevel, useAudio } from "../audio/store";
import type { AudioApp } from "../audio/types";
import { arrange, type Arrangement, isPinned, isUnity, listedApps, moveBy, neighbor, togglePin } from "./arrangement";
import { S } from "./strings";

const MASTER_WIDTH = 72;
const COLUMN_WIDTH = 96;
/** NotchLayout.mixerMinimumHeight. */
const MINIMUM_HEIGHT = 104;

/** Both the output and app columns reserve the same header, gaps and footer. */
export const trackHeight = (height: number) => Math.min(160, Math.max(32, height - 32 - 28 - 6 * 2));

export const useMixerUi = create<{ options: boolean }>(() => ({ options: false }));
const toggleOptions = () => useMixerUi.setState((s) => ({ options: !s.options }));

const saveArrangement = (a: Arrangement) => void setPrefs({ island: { mixerAppArrangement: a } });

/** Hides an app from the list, or with null lists it again: a null removes the key from the stored map. */
const setHidden = (key: string, name: string | null) =>
  void setPrefs({ island: { mixerHiddenApps: { [key]: name } as Record<string, string> } });

// MARK: Band

function deviceEntries(devices: { id: string; name: string; isDefault: boolean }[]): MenuEntry[] {
  return devices.map((d) => ({
    label: d.name,
    checked: d.isDefault,
    onSelect: () => void useAudio.getState().setDefault(d.id),
  }));
}

/** One line naming the current output, or the switch error; a menu where switching works. */
export function MixerLeading() {
  const outputs = useAudio((s) => s.state.outputs);
  const canSwitch = useAudio((s) => s.state.canSwitch);
  const error = useAudio((s) => (s.switchError?.input === false ? s.switchError.message : null));
  const label = error ? S.couldNotSwitch(error) : (outputs.find((d) => d.isDefault)?.name ?? S.outputUnavailable);
  if (!canSwitch || outputs.length === 0)
    return (
      <span className="mx-band-label" title={label}>
        {label}
      </span>
    );
  return (
    <MenuButton
      className="mx-band-menu"
      label={S.chooseOutput}
      alignRight={false}
      entries={() => deviceEntries(outputs)}
    >
      <span className="mx-band-label">{label}</span>
      <ChevronDown size={11} strokeWidth={2.5} aria-hidden="true" />
    </MenuButton>
  );
}

export function MixerTrailing() {
  const open = useMixerUi((s) => s.options);
  return (
    <IconButton
      icon={open ? X : SlidersHorizontal}
      label={open ? S.close : S.options}
      selected={open}
      onClick={toggleOptions}
    />
  );
}

// MARK: Page

export function MixerPage({ width, height }: PageProps) {
  const options = useMixerUi((s) => s.options);
  const supported = useAudio((s) => s.state.supported);
  const received = useAudio((s) => s.received);
  const hasMic = useAudio((s) => s.state.input.volume !== null);
  usePageLayer(options, () => useMixerUi.setState({ options: false }));

  // Apps and their peaks are read four times a second only while the page shows.
  useEffect(() => {
    latest("watch", () => bridge.audio.watchApps(true));
    return () => {
      latest("watch", () => bridge.audio.watchApps(false));
      useMixerUi.setState({ options: false });
    };
  }, []);

  const faderHeight = Math.max(MINIMUM_HEIGHT, height);
  const leading = hasMic ? MASTER_WIDTH * 2 + 10 : MASTER_WIDTH;
  if (options) return <MixerOptions />;
  return (
    <div className="mx-desk" style={{ "--mx-track": `${trackHeight(faderHeight)}px` } as CSSProperties}>
      <MasterFader height={faderHeight} />
      {hasMic ? <MicrophoneFader height={faderHeight} /> : null}
      <div className="mx-divider" aria-hidden="true" />
      <div className="mx-apps" style={{ width: Math.max(0, width - leading - 21) }}>
        {supported ? (
          <AppRail height={faderHeight} />
        ) : received ? (
          <EmptyState icon={Volume2} title={S.unavailable} />
        ) : null}
      </div>
    </div>
  );
}

function MuteButton(props: { muted: boolean; label: string; disabled?: boolean; onClick(): void; mic?: boolean }) {
  const Icon = props.mic ? (props.muted ? MicOff : Mic) : props.muted ? VolumeX : Volume2;
  return (
    <button
      type="button"
      className={`mx-mute${props.muted ? " is-muted" : ""}`}
      aria-label={props.label}
      title={props.label}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      <Icon key={props.muted ? "off" : "on"} size={16} strokeWidth={2.25} />
    </button>
  );
}

/** The system output: its mute, level and percent, in the apps' column shape. */
function MasterFader({ height }: { height: number }) {
  const output = useAudio((s) => s.state.output);
  const level = shownLevel(output);
  const muted = output.muted === true;
  const set = useAudio.getState().setOutput;
  return (
    <div className="mx-fader" style={{ width: MASTER_WIDTH, height }}>
      <MuteButton
        muted={muted}
        label={muted ? S.unmute : S.mute}
        disabled={output.muted === null}
        onClick={() => set({ muted: !muted })}
      />
      {level === null ? (
        <span className="mx-note">{S.outputUnavailable}</span>
      ) : (
        <>
          <Slider
            vertical
            thickness={28}
            className="mx-track"
            label={S.output}
            value={level}
            valueText={`${percent(level)}%`}
            onChange={(v) => set({ volume: v })}
          />
          <EditablePercent
            value={percent(level)}
            max={100}
            label={S.output}
            onCommit={(v) => set({ volume: v / 100 })}
          />
        </>
      )}
    </div>
  );
}

/** The microphone beside the output; Mic Mute here mutes every microphone. */
function MicrophoneFader({ height }: { height: number }) {
  const input = useAudio((s) => s.state.input);
  const micMuted = useAudio((s) => s.state.micMuted);
  const level = input.volume ?? 0;
  const set = useAudio.getState().setInput;
  return (
    <div className="mx-fader" style={{ width: MASTER_WIDTH, height }}>
      <MuteButton
        mic
        muted={micMuted}
        label={micMuted ? S.micUnmute : S.micMute}
        onClick={() => useAudio.getState().toggleMicMute()}
      />
      <Slider
        vertical
        thickness={28}
        className="mx-track"
        label={S.microphone}
        value={level}
        valueText={`${percent(level)}%`}
        disabled={micMuted}
        onChange={(v) => set({ volume: v })}
      />
      <EditablePercent
        value={percent(level)}
        max={100}
        label={S.microphone}
        disabled={micMuted}
        onCommit={(v) => set({ volume: v / 100 })}
      />
    </div>
  );
}

function AppRail({ height }: { height: number }) {
  const all = useAudio((s) => s.state.apps);
  const arrangement = useIsland((s) => s.settings.mixerAppArrangement);
  const hidden = useIsland((s) => s.settings.mixerHiddenApps);
  const hideInactive = useIsland((s) => s.settings.mixerHideInactiveApps);
  const apps = listedApps(all, arrangement, hidden, hideInactive);
  const ids = apps.map((a) => a.key);
  const reorder = useReorder({
    ids,
    axis: "x",
    requireModifier: true,
    allowed: (moving, target) => isPinned(arrangement, moving) === isPinned(arrangement, target),
    onReorder: (next) => saveArrangement(arrange(arrangement, ids, next)),
  });
  if (apps.length === 0) return <EmptyState icon={Volume2} title={S.empty} />;
  return (
    <div className="mx-rail">
      {apps.map((app) => (
        <div
          key={app.key}
          className={`mx-column-slot${reorder.moving === app.key ? " is-moving" : ""}`}
          style={{ width: COLUMN_WIDTH }}
          title={`${app.name}\n${S.arrange}`}
          {...reorder.itemProps(app.key)}
        >
          <AppColumn app={app} height={height} arrangement={arrangement} visible={ids} />
        </div>
      ))}
    </div>
  );
}

/** Icons by exe path, kept for the session. */
const icons = new Map<string, string | null>();

function useAppIcon(path: string | null): string | null {
  const [icon, setIcon] = useState(() => (path ? (icons.get(path) ?? null) : null));
  useEffect(() => {
    if (!path || icons.has(path)) return;
    let live = true;
    void bridge.shell.icon(path, Math.round(28 * window.devicePixelRatio)).then((url) => {
      icons.set(path, url ?? null);
      if (live) setIcon(url ?? null);
    });
    return () => {
      live = false;
    };
  }, [path]);
  return icon;
}

function AppColumn(props: { app: AudioApp; height: number; arrangement: Arrangement; visible: string[] }) {
  const { app, arrangement, visible } = props;
  const icon = useAppIcon(app.path);
  const menu = useMenu();
  const pinned = isPinned(arrangement, app.key);
  const level = app.muted ? 0 : app.volume;
  const value = percent(level);
  const set = (change: { volume?: number; muted?: boolean }) => useAudio.getState().setApp(app.key, change);
  const actions = (): MenuEntry[] => [
    {
      label: pinned ? S.unpin : S.pinFirst,
      icon: pinned ? PinOff : Pin,
      onSelect: () => saveArrangement(togglePin(arrangement, app.key)),
    },
    {
      label: S.moveLeft,
      icon: ArrowLeft,
      disabled: neighbor(arrangement, app.key, -1, visible) === null,
      onSelect: () => saveArrangement(moveBy(arrangement, app.key, -1, visible)),
    },
    {
      label: S.moveRight,
      icon: ArrowRight,
      disabled: neighbor(arrangement, app.key, 1, visible) === null,
      onSelect: () => saveArrangement(moveBy(arrangement, app.key, 1, visible)),
    },
    { kind: "separator" },
    { label: S.reset, disabled: isUnity(app.volume) && !app.muted, onSelect: () => set({ volume: 1, muted: false }) },
    { kind: "separator" },
    {
      label: S.hideFromList,
      icon: EyeOff,
      onSelect: () => setHidden(app.key, app.name),
    },
  ];
  return (
    <div
      className="mx-fader"
      style={{ height: props.height }}
      aria-label={app.name}
      onContextMenu={(e) => {
        e.preventDefault();
        menu.open({ x: e.clientX, y: e.clientY }, actions());
      }}
    >
      <div className="mx-head">
        {pinned ? <Pin className="mx-pin" size={9} aria-label={S.pinFirst} /> : null}
        <span className="mx-icon">
          {icon ? (
            <img src={icon} alt="" draggable={false} />
          ) : (
            <Volume2 size={18} strokeWidth={2} aria-hidden="true" />
          )}
          {app.playing ? <span className="mx-playing" aria-hidden="true" /> : null}
        </span>
        <MenuButton className="mx-actions" label={`${S.actions}: ${app.name}`} entries={actions}>
          <Ellipsis size={13} strokeWidth={2.5} aria-hidden="true" />
        </MenuButton>
      </div>
      <Slider
        vertical
        thickness={28}
        className="mx-track"
        label={app.name}
        value={level}
        valueText={`${value}%`}
        onChange={(v) => set({ volume: v })}
      />
      <div className="mx-foot">
        <button
          type="button"
          className="mx-foot-button is-reset"
          aria-label={S.reset}
          title={S.reset}
          disabled={value === 100}
          style={{ opacity: value === 100 ? 0 : 1 }}
          onClick={() => set({ volume: 1, muted: false })}
        >
          <RotateCcw size={12} strokeWidth={2.5} />
        </button>
        <EditablePercent value={value} max={100} label={app.name} onCommit={(v) => set({ volume: v / 100 })} />
        <button
          type="button"
          className={`mx-foot-button${app.muted || app.volume <= 0.001 ? " is-muted" : ""}`}
          aria-label={app.muted ? S.unmute : S.mute}
          title={app.muted ? S.unmute : S.mute}
          onClick={() => set({ muted: !app.muted })}
        >
          {app.muted || app.volume <= 0.001 ? (
            <VolumeX size={12} strokeWidth={2.5} />
          ) : (
            <Volume2 size={12} strokeWidth={2.5} />
          )}
        </button>
      </div>
      {menu.menu}
    </div>
  );
}

// MARK: Options

/**
 * Everything the desk leaves out, in one list that scrolls: the output, the
 * microphone and its level, and the list's own options.
 */
function MixerOptions() {
  const state = useAudio((s) => s.state);
  const switchError = useAudio((s) => s.switchError);
  const hideInactive = useIsland((s) => s.settings.mixerHideInactiveApps);
  const hidden = useIsland((s) => s.settings.mixerHiddenApps);
  const output = state.outputs.find((d) => d.isDefault)?.name ?? S.outputUnavailable;
  const microphone = state.inputs.find((d) => d.isDefault)?.name ?? S.noMicrophones;
  const hiddenCount = Object.keys(hidden).length;
  const listEntries = (): MenuEntry[] => {
    const listed = state.apps.filter((a) => !(a.key in hidden)).map((a) => ({ key: a.key, name: a.name, shown: true }));
    const off = Object.entries(hidden).map(([key, name]) => ({ key, name, shown: false }));
    return [...listed, ...off]
      .sort((a, b) => a.name.localeCompare(b.name))
      .map((app) => ({
        label: app.name,
        checked: app.shown,
        onSelect: () => setHidden(app.key, app.shown ? app.name : null),
      }));
  };
  return (
    <div className="mx-options">
      <Row icon={<Volume2 size={11} strokeWidth={2.25} />} title={S.output}>
        <DeviceMenu label={S.chooseOutput} current={output} devices={state.outputs} canSwitch={state.canSwitch} />
      </Row>
      {state.outputs.length === 0 ? <p className="mx-message">{S.noOutputs}</p> : null}
      {switchError && !switchError.input ? <p className="mx-message">{S.couldNotSwitch(switchError.message)}</p> : null}
      <Row
        icon={state.micMuted ? <MicOff size={11} strokeWidth={2.25} /> : <Mic size={11} strokeWidth={2.25} />}
        title={S.microphone}
      >
        <DeviceMenu
          label={S.chooseMicrophone}
          current={microphone}
          devices={state.inputs}
          canSwitch={state.canSwitch}
        />
      </Row>
      {state.input.volume !== null ? (
        <div className="mx-option-row">
          <button
            type="button"
            className={`mx-small-mute${state.micMuted ? " is-muted" : ""}`}
            aria-label={state.micMuted ? S.micUnmute : S.micMute}
            title={state.micMuted ? S.micUnmute : S.micMute}
            onClick={() => useAudio.getState().toggleMicMute()}
          >
            {state.micMuted ? <MicOff size={11} strokeWidth={2.25} /> : <Mic size={11} strokeWidth={2.25} />}
          </button>
          <Slider
            label={S.microphone}
            value={state.input.volume}
            disabled={state.micMuted}
            valueText={`${percent(state.input.volume)}%`}
            onChange={(v) => useAudio.getState().setInput({ volume: v })}
          />
          <EditablePercent
            className="is-small"
            value={percent(state.input.volume)}
            max={100}
            label={S.microphone}
            disabled={state.micMuted}
            onCommit={(v) => useAudio.getState().setInput({ volume: v / 100 })}
          />
        </div>
      ) : null}
      {state.inputs.length === 0 ? <p className="mx-message">{S.noMicrophones}</p> : null}
      {switchError?.input ? <p className="mx-message">{S.couldNotSwitch(switchError.message)}</p> : null}
      <div className="mx-options-divider" />
      <div className="mx-option-row">
        <span className="mx-option-title is-primary">{S.hideInactive}</span>
        <Switch
          label={S.hideInactive}
          on={hideInactive}
          onChange={(on) => void setPrefs({ island: { mixerHideInactiveApps: on } })}
        />
      </div>
      <div className="mx-option-row">
        <span className="mx-option-title">{S.visibleApps}</span>
        <MenuButton className="mx-device-menu" label={S.visibleApps} entries={listEntries}>
          <span className="mx-device-name">{hiddenCount === 0 ? S.allShown : S.hidden(hiddenCount)}</span>
          <ChevronDown size={10} strokeWidth={2.5} aria-hidden="true" />
        </MenuButton>
      </div>
    </div>
  );
}

function Row(props: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <div className="mx-option-row">
      <span className="mx-option-icon" aria-hidden="true">
        {props.icon}
      </span>
      <span className="mx-option-title">{props.title}</span>
      {props.children}
    </div>
  );
}

function DeviceMenu(props: {
  label: string;
  current: string;
  devices: { id: string; name: string; isDefault: boolean }[];
  canSwitch: boolean;
}) {
  if (!props.canSwitch || props.devices.length === 0)
    return (
      <span className="mx-device-name is-static" title={props.current}>
        {props.current}
      </span>
    );
  return (
    <MenuButton className="mx-device-menu" label={props.label} entries={() => deviceEntries(props.devices)}>
      <span className="mx-device-name">{props.current}</span>
      <ChevronDown size={10} strokeWidth={2.5} aria-hidden="true" />
    </MenuButton>
  );
}
