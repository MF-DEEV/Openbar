// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A shortcut that records the next combination when clicked
// (ShortcutRecorderButton.swift): it reads "Press the new shortcut" until a
// key with Ctrl, Alt or Win comes, and Escape or a click elsewhere keeps
// the old one.

import { useEffect, useRef, useState } from "react";
import { usePageLayer } from "../../island/pageLayer";
import { acceleratorFrom, formatAccelerator } from "../shortcut";

export function ShortcutField(props: {
  value: string;
  onChange(accelerator: string): void;
  label: string;
  recordingLabel: string;
  /** Told when recording starts and stops, so the shortcut's owner can let go of it meanwhile. */
  onRecording?(on: boolean): void;
  className?: string;
  disabled?: boolean;
}) {
  const [recording, setRecording] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const latest = useRef(props);
  latest.current = props;
  usePageLayer(recording, () => setRecording(false));

  useEffect(() => {
    if (!recording) return;
    latest.current.onRecording?.(true);
    const key = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "Escape") {
        setRecording(false);
        return;
      }
      const accelerator = acceleratorFrom(e);
      if (!accelerator) return;
      setRecording(false);
      latest.current.onChange(accelerator);
    };
    const away = (e: PointerEvent) => {
      if (!button.current?.contains(e.target as Node)) setRecording(false);
    };
    const blur = () => setRecording(false);
    window.addEventListener("keydown", key, true);
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("blur", blur);
      latest.current.onRecording?.(false);
    };
  }, [recording]);

  const shown = formatAccelerator(props.value);
  return (
    <button
      ref={button}
      type="button"
      className={`u-shortcut${recording ? " is-recording" : ""}${props.className ? " " + props.className : ""}`}
      aria-label={recording ? props.recordingLabel : `${props.label}: ${shown}`}
      aria-pressed={recording}
      disabled={props.disabled}
      onClick={() => setRecording((r) => !r)}
    >
      {recording ? props.recordingLabel : shown}
    </button>
  );
}
