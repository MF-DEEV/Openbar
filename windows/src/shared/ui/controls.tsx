// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Small shared atoms for section pages: the control surface card
// (NotchControlSurface), a switch, and the editable percent (NotchMixerView).

import { type CSSProperties, type ReactNode, useEffect, useRef, useState } from "react";

/** White 6.5% card (12% when selected); radius per use. */
export function Surface(props: {
  children?: ReactNode;
  selected?: boolean;
  radius?: number;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <div
      className={`u-surface${props.selected ? " is-selected" : ""}${props.className ? " " + props.className : ""}`}
      style={{ borderRadius: props.radius ?? 14, ...props.style }}
    >
      {props.children}
    </div>
  );
}

/** The small on/off switch used inside island option cards. */
export function Switch(props: { on: boolean; onChange(on: boolean): void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={props.on}
      aria-label={props.label}
      title={props.label}
      disabled={props.disabled}
      className={`u-switch${props.on ? " is-on" : ""}`}
      onClick={() => props.onChange(!props.on)}
    >
      <span className="u-switch-knob" />
    </button>
  );
}

/** A typed percent: "40", "40 %" or "40,5"; null for anything else, an empty
 * field included, which leaves the value as it was (MixerRoutingSupport.volumeFraction). */
export function parsePercent(draft: string): number | null {
  const text = draft.replace(/[%\s]/g, "").replace(",", ".");
  if (text === "") return null;
  const n = Number(text);
  return Number.isFinite(n) ? n : null;
}

/** "n%" that turns into a field on click; a typed number is clamped to 0…max. */
export function EditablePercent(props: {
  value: number;
  max: number;
  onCommit(value: number): void;
  label: string;
  disabled?: boolean;
  className?: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (editing) input.current?.select();
  }, [editing]);
  const shown = Math.round(props.value);
  const commit = () => {
    const n = parsePercent(draft);
    if (n !== null) props.onCommit(Math.min(props.max, Math.max(0, n)));
    setEditing(false);
  };
  if (editing)
    return (
      <input
        ref={input}
        className={`u-percent is-editing${props.className ? " " + props.className : ""}`}
        aria-label={props.label}
        value={draft}
        inputMode="numeric"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") commit();
          else if (e.key === "Escape") setEditing(false);
        }}
      />
    );
  return (
    <button
      type="button"
      className={`u-percent${props.className ? " " + props.className : ""}`}
      aria-label={`${props.label}: ${shown}%`}
      disabled={props.disabled}
      onClick={() => {
        setDraft(String(shown));
        setEditing(true);
      }}
    >
      {shown}%
    </button>
  );
}
