// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The home page's shortcut tiles (NotchActionTile and the tiles built on it,
// NotchControlsView.swift:389-580; spec 05 §1.1-§1.2): a 40 pt circle over
// a two-line label, 76 × 74. A live reading takes the first line.

import { useLocale, t } from "../i18n";

import {
  Calendar,
  Camera,
  Coffee,
  Gauge,
  type LucideIcon,
  Mic,
  MicOff,
  NotebookText,
  Timer,
  Video,
  Volume2,
} from "lucide-react";

import type { ModuleId } from "../island/modules";
import { useIsland } from "../island/store";
import { RollingDigits } from "../shared/ui/RollingDigits";
import { useAudio } from "../audio/store";
import { toggleKeepAwake, useKeepAwake } from "./keepAwake";
import { showNetworkReading } from "../system/store";
import { S } from "./strings";
import { type TileReading, useTileRegistry } from "./tiles";
import type { ControlItem, TileAccent } from "./types";

/** A title longer than this gives way to the compact one under a reading (about 68 pt at 11 pt). */
const READING_TITLE_CHARS = 12;

export function ActionTile(props: {
  icon: LucideIcon;
  title: string;
  reading?: string;
  compactTitle?: string;
  active?: boolean;
  accent?: TileAccent;
  onSelect(): void;
}) {
  useLocale();
  const Icon = props.icon;
  const under = props.compactTitle && props.title.length > READING_TITLE_CHARS ? props.compactTitle : props.title;
  return (
    <button
      type="button"
      className={`ct-tile${props.active ? " is-active" : ""}`}
      data-accent={props.accent ?? "selection"}
      aria-label={props.reading ? `${props.title}, ${props.reading}` : props.title}
      aria-pressed={props.active}
      title={props.title}
      onClick={props.onSelect}
    >
      <span className="ct-tile-visual">
        <span className="ct-tile-circle">
          <Icon key={Icon.displayName} size={18} strokeWidth={2} aria-hidden="true" />
        </span>
      </span>
      <span className="ct-tile-label">
        {props.reading ? (
          <>
            <RollingDigits className="ct-tile-reading" text={props.reading} />
            <span className="ct-tile-under">{under}</span>
          </>
        ) : (
          <span className="ct-tile-title">{props.title}</span>
        )}
      </span>
    </button>
  );
}

const select = (module: ModuleId) => () => useIsland.getState().select(module);

/**
 * Collapses, then asks for a screenshot. The Tools engineer's capture code
 * listens for "vorssaint:screenshot" and waits for the island to settle
 * before it shoots (service.perform, spec 05 §0).
 */
function recording() {
  useIsland.getState().collapse();
  window.dispatchEvent(new CustomEvent("vorssaint:recording"));
}

function screenshot() {
  useIsland.getState().collapse();
  window.dispatchEvent(new CustomEvent("vorssaint:screenshot"));
}

function KeepAwakeTile() {
  useLocale();
  const active = useKeepAwake((s) => s.state.active);
  return (
    <ActionTile
      icon={Coffee}
      title={S.keepAwake}
      active={active}
      accent="awake"
      onSelect={() => void toggleKeepAwake()}
    />
  );
}

function MicrophoneTile() {
  useLocale();
  const muted = useAudio((s) => s.state.micMuted);
  return (
    <ActionTile
      icon={muted ? MicOff : Mic}
      title={muted ? S.micUnmute : S.micMute}
      active={muted}
      accent="alert"
      onSelect={() => useAudio.getState().toggleMicMute()}
    />
  );
}

/** A tile another section gives a reading; its hook runs on every render. */
function ReadingTile(props: { hook: () => TileReading | null; icon: LucideIcon; title: string; onSelect(): void }) {
  useLocale();
  const r = props.hook();
  return (
    <ActionTile
      icon={r?.icon ?? props.icon}
      title={r?.title ?? props.title}
      reading={r?.reading}
      compactTitle={r?.compactTitle}
      active={r?.active}
      accent={r?.accent}
      onSelect={r?.onSelect ?? props.onSelect}
    />
  );
}

/** The plain tiles: glyph, title and what a click does. */
const PLAIN: Partial<Record<ControlItem, { icon: LucideIcon; title: () => string; onSelect(): void }>> = {
  mixer: {
    icon: Volume2,
    title: () => {
      useLocale();
      return S.mixer;
    },
    onSelect: select("mixer"),
  },
  timer: {
    icon: Timer,
    title: () => {
      useLocale();
      return t("module.timer");
    },
    onSelect: select("timer"),
  },
  calendar: {
    icon: Calendar,
    title: () => {
      useLocale();
      return t("module.calendar");
    },
    onSelect: select("calendar"),
  },
  screenshot: {
    icon: Camera,
    title: () => {
      useLocale();
      return S.screenshot;
    },
    onSelect: screenshot,
  },
  recording: {
    icon: Video,
    title: () => {
      useLocale();
      return S.recording;
    },
    onSelect: recording,
  },
  // The tile opens System's network reading, where the speed test is (NotchControlsView showMetric(.network)).
  speedTest: {
    icon: Gauge,
    title: () => {
      useLocale();
      return S.speedTest;
    },
    onSelect: showNetworkReading,
  },
  scratchpad: {
    icon: NotebookText,
    title: () => {
      useLocale();
      return S.scratchpad;
    },
    onSelect: select("scratchpad"),
  },
};

export function ShortcutTile({ item }: { item: ControlItem }) {
  useLocale();
  const registered = useTileRegistry((s) => s.hooks[item]);
  if (item === "keepAwake") return <KeepAwakeTile />;
  if (item === "microphone") return <MicrophoneTile />;
  const plain = PLAIN[item];
  if (!plain) return null;
  if (registered)
    return (
      <ReadingTile
        key={registered.id}
        hook={registered.hook}
        icon={plain.icon}
        title={plain.title()}
        onSelect={plain.onSelect}
      />
    );
  return <ActionTile icon={plain.icon} title={plain.title()} onSelect={plain.onSelect} />;
}
