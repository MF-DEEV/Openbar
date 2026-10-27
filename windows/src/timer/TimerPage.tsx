// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Timer page (NotchTimerView.swift; spec 04 §4.2), tinted orange. The
// island's top row holds the modes and Start while setting up, the phase
// and the way out once a session runs. Below, the ruler and its clock, or
// the round button and the large clock.

import { ChevronDown, Pause, Play, X } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import { useIsland } from "../island/store";
import type { PageDefinition, PageProps } from "../island/views/Expanded";
import { IconButton } from "../island/views/Controls";
import { MenuButton, type MenuEntry } from "../shared/ui/Menu";
import { RollingDigits } from "../shared/ui/RollingDigits";
import * as model from "./model";
import { setTimerPref, timerPref, useTimerPref, type TimerPref } from "./prefs";
import { Ruler } from "./Ruler";
import { modeGlyph, textWidth } from "./TimerStrip";
import { sessionTitle, timer, useTimer } from "./store";
import { modeTitle, optionTitle, phaseTitle, T } from "./strings";
import "./timer.css";

/** NotchLayout: the ruler row, its least height, and the Pomodoro's readouts under it. */
export const RULER_ROW = 82;
const MINIMUM_RULER_ROW = 56;
const ROW_SPACING = 8;
const READOUTS_ROW = 28;

/** The page's height: the ruler row, plus the readouts while a Pomodoro is set up (NotchLayout.timer). */
export const pageHeight = (mode: model.TimerMode, session: boolean) =>
  RULER_ROW + (!session && mode === "pomodoro" ? ROW_SPACING + READOUTS_ROW : 0);

/** The mode picker (NotchTimerSupport.ModePicker). */
const PICKER = { label: 12, padding: 4, spacing: 14, menuSpacing: 3, chevron: 9, minimumScale: 0.7 };
const LABEL_FONT = `500 ${PICKER.label}px 'Inter Variable', Inter, sans-serif`;
const CLOCK_FONT = (size: number) => `200 ${size}px 'Inter Variable', Inter, sans-serif`;

function Page({ width, height }: PageProps) {
  const session = useTimer((s) => s.session);
  return model.hasSession(session) ? <Active width={width} /> : <Setup width={width} height={height} />;
}

// MARK: Setup

const RULED: Partial<Record<model.TimerMode, "timerMinutes" | "pomodoroFocusMinutes">> = {
  timer: "timerMinutes",
  pomodoro: "pomodoroFocusMinutes",
};

function Setup({ width, height }: { width: number; height: number }) {
  const mode = useTimerPref("timerMode");
  const key = RULED[mode];
  const minutes = useTimerPref(key ?? "timerMinutes");
  const below = mode === "pomodoro" ? ROW_SPACING + READOUTS_ROW : 0;
  const rulerRow = below ? Math.min(RULER_ROW, Math.max(MINIMUM_RULER_ROW, height - below)) : RULER_ROW;
  const value =
    key === "pomodoroFocusMinutes" ? model.clampOption("focusMinutes", minutes) : model.timerMinutes(minutes);
  const clock = key ? model.clockText(value * 60) : model.stopwatchText(0);

  return (
    <div className="tm-setup">
      <div className="tm-ruler-row" style={{ height: rulerRow }}>
        {key ? (
          <Ruler
            value={value}
            height={rulerRow}
            label={mode === "pomodoro" ? T.focus : T.timer}
            onChange={(next) => setTimerPref(key, next)}
          />
        ) : (
          <span className="tm-spacer" />
        )}
        <Clock text={clock} size={26} room={width} />
      </div>
      {mode === "pomodoro" ? <Readouts /> : null}
    </div>
  );
}

const READOUTS: { option: model.PomodoroOption; pref: TimerPref; duration: boolean }[] = [
  { option: "shortBreakMinutes", pref: "pomodoroShortBreakMinutes", duration: true },
  { option: "longBreakMinutes", pref: "pomodoroLongBreakMinutes", duration: true },
  { option: "longBreakInterval", pref: "pomodoroLongBreakInterval", duration: false },
  { option: "totalSessions", pref: "pomodoroTotalSessions", duration: false },
];

/** The breaks and sessions as readouts: the name small and dim, the value in orange, every allowed value in a menu. */
function Readouts() {
  return (
    <div className="tm-readouts" style={{ height: READOUTS_ROW }}>
      {READOUTS.map((r) => (
        <Readout key={r.option} {...r} />
      ))}
    </div>
  );
}

function Readout({ option, pref, duration }: (typeof READOUTS)[number]) {
  const current = model.clampOption(option, useTimerPref(pref) as number);
  const title = optionTitle(option);
  const display = (n: number) => (duration ? model.durationText(n * 60) : String(n));
  const [low, high] = model.POMODORO_RANGES[option];
  const entries = (): MenuEntry[] =>
    Array.from({ length: high - low + 1 }, (_, i) => low + i).map((n) => ({
      label: display(n),
      checked: n === current,
      onSelect: () => setTimerPref(pref, n),
    }));
  return (
    <MenuButton className="tm-readout" label={`${title}: ${display(current)}`} entries={entries} alignRight={false}>
      <span className="tm-readout-name">{title}</span>
      <span className="tm-readout-value">
        {display(current)}
        <ChevronDown size={9} strokeWidth={2.5} aria-hidden />
      </span>
    </MenuButton>
  );
}

// MARK: Running

function Active({ width }: { width: number }) {
  const session = useTimer((s) => s.session);
  const now = useTimer((s) => s.now);
  const text = model.clockTextFor(session, now);
  let button = null;
  if (!session.completed) {
    const paused = model.isPaused(session);
    button = (
      <RoundButton icon={paused ? Play : Pause} label={paused ? T.resume : T.pause} onClick={timer.pauseOrResume} />
    );
  } else if (model.canStartNext(session)) {
    button = (
      <RoundButton icon={Play} label={`${T.start} ${phaseTitle(model.nextPhase(session))}`} onClick={timer.startNext} />
    );
  }
  return (
    <div className="tm-active" style={{ height: RULER_ROW }}>
      {button}
      <span className="tm-spacer" />
      <Clock text={text} size={62} room={width - (button ? 52 + 16 : 0)} rolling label={sessionTitle(session)} />
    </div>
  );
}

function RoundButton(props: { icon: typeof Play; label: string; onClick(): void }) {
  const Icon = props.icon;
  return (
    <button type="button" className="tm-round" aria-label={props.label} title={props.label} onClick={props.onClick}>
      <Icon size={23} strokeWidth={2.5} fill="currentColor" />
    </button>
  );
}

/** A thin orange clock that narrows to half its size before it would overflow. */
function Clock(props: { text: string; size: number; room: number; rolling?: boolean; label?: string }) {
  const natural = textWidth(model.readingShape(props.text), CLOCK_FONT(props.size));
  const size = natural > props.room ? Math.max(props.size * 0.5, (props.size * props.room) / natural) : props.size;
  const style = { fontSize: size };
  if (!props.rolling)
    return (
      <span className="tm-clock" style={style}>
        {props.text}
      </span>
    );
  return (
    <span className="tm-clock" style={style} role="timer" aria-label={props.label}>
      <RollingDigits text={props.text} label={props.text} />
    </span>
  );
}

// MARK: Top row

/** The modes while setting up; the phase and the cycle's place once running. */
function Leading() {
  const session = useTimer((s) => s.session);
  return model.hasSession(session) ? <PhaseReading session={session} /> : <ModePicker />;
}

function PhaseReading({ session }: { session: model.TimerSession }) {
  const title = sessionTitle(session);
  const progress =
    session.mode === "pomodoro" ? T.sessionProgress(model.sessionNumber(session), session.config.totalSessions) : null;
  return (
    <div className="tm-phase" title={[title, progress].filter(Boolean).join(", ")}>
      <span className="tm-phase-title">{title}</span>
      {progress ? <span className="tm-phase-progress">{progress}</span> : null}
    </div>
  );
}

/**
 * Three words, the current one underlined in orange, where the row leaves
 * them room; otherwise a menu named by the current mode, narrowing to 0.7 of
 * its size, or showing the mode's glyph when even that doesn't fit.
 */
function ModePicker() {
  const mode = useTimerPref("timerMode");
  const slot = useRef<HTMLDivElement>(null);
  const [room, setRoom] = useState(Infinity);
  useLayoutEffect(() => {
    const el = slot.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setRoom(el.clientWidth));
    observer.observe(el);
    setRoom(el.clientWidth);
    return () => observer.disconnect();
  }, []);
  const widths = model.TIMER_MODES.map((m) => textWidth(modeTitle(m), LABEL_FONT));
  const words = widths.reduce((sum, w) => sum + w + 2 * PICKER.padding, 0) + PICKER.spacing * (widths.length - 1) + 2;
  return (
    <div ref={slot} className="tm-modes-slot">
      {words <= room ? <ModeWords mode={mode} /> : <ModeMenu mode={mode} room={room} widest={Math.max(...widths)} />}
    </div>
  );
}

function ModeWords({ mode }: { mode: model.TimerMode }) {
  const row = useRef<HTMLDivElement>(null);
  const [line, setLine] = useState<{ left: number; width: number } | null>(null);
  useLayoutEffect(() => {
    const button = row.current?.querySelector<HTMLElement>(`[data-mode="${mode}"]`);
    if (button) setLine({ left: button.offsetLeft + PICKER.padding, width: button.offsetWidth - 2 * PICKER.padding });
  }, [mode]);
  return (
    <div ref={row} className="tm-modes" role="radiogroup" aria-label={T.timer}>
      {model.TIMER_MODES.map((m) => (
        <button
          key={m}
          type="button"
          role="radio"
          aria-checked={m === mode}
          data-mode={m}
          className={`tm-mode${m === mode ? " is-selected" : ""}`}
          onClick={() => setTimerPref("timerMode", m)}
        >
          {modeTitle(m)}
        </button>
      ))}
      {line ? (
        <span className="tm-mode-line" style={{ width: line.width, transform: `translateX(${line.left}px)` }} />
      ) : null}
    </div>
  );
}

function ModeMenu({ mode, room, widest }: { mode: model.TimerMode; room: number; widest: number }) {
  const chrome = PICKER.menuSpacing + PICKER.chevron + 2 * PICKER.padding;
  const named = widest * PICKER.minimumScale * 1.05 + chrome <= room;
  const current = modeTitle(mode);
  const scale = Math.min(1, Math.max(PICKER.minimumScale, (room - chrome) / textWidth(current, LABEL_FONT)));
  const Glyph = modeGlyph(mode);
  return (
    <MenuButton
      className="tm-mode-menu"
      label={current}
      alignRight={false}
      entries={() =>
        model.TIMER_MODES.map((m) => ({
          label: modeTitle(m),
          checked: m === mode,
          onSelect: () => setTimerPref("timerMode", m),
        }))
      }
    >
      {named ? (
        <span style={{ fontSize: PICKER.label * scale }}>{current}</span>
      ) : (
        <Glyph size={PICKER.label + 2} strokeWidth={2} aria-hidden />
      )}
      <ChevronDown size={PICKER.chevron + 2} strokeWidth={2.5} aria-hidden />
    </MenuButton>
  );
}

/** Start while setting up; ✕ (Cancel, or Done once finished) while a session exists. */
function Trailing() {
  const session = useTimer((s) => s.session);
  const mode = useTimerPref("timerMode");
  if (model.hasSession(session)) {
    return <IconButton icon={X} label={session.completed ? T.done : T.cancel} onClick={timer.cancel} />;
  }
  return (
    <button
      type="button"
      className="tm-start"
      title={mode === "pomodoro" ? T.pomodoroHint : T.start}
      onClick={() => {
        const current = timerPref("timerMode");
        timer.start(current, timerPref(current === "pomodoro" ? "pomodoroFocusMinutes" : "timerMinutes"));
      }}
    >
      {T.start}
    </button>
  );
}

/** Keeps the page as tall as what it shows (NotchLayout.timer). */
export function syncPageHeight() {
  const island = useIsland.getState();
  const session = model.hasSession(useTimer.getState().session);
  island.setPageContent("timer", pageHeight(timerPref("timerMode"), session));
}

export const timerPage: PageDefinition = { Page, Leading, Trailing };
