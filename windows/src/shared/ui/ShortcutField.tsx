// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A shortcut that records the next combination when clicked
// (ShortcutRecorderButton.swift): it reads "Press keys" until a key with
// Ctrl, Alt or Win comes, and Escape or a click elsewhere keeps the old one.
// A key without them is refused through `onInvalid`.

import { useEffect, useRef, useState } from "react";
import { usePageLayer } from "../../island/pageLayer";
import { acceleratorFrom, formatAccelerator, needsModifier } from "../shortcut";

export function ShortcutField(props: {
  value: string;
  onChange(accelerator: string): void | Promise<void>;
  label: string;
  recordingLabel: string;
  /** Told when recording starts and stops, so the shortcut's owner can let go of it meanwhile. */
  onRecording?(on: boolean): void | Promise<void>;
  onError?(error: unknown): void;
  /** Settings may show an explicit action beside the configured keys. */
  actionLabel?: string;
  /** A key came without Ctrl, Alt or Win; recording goes on. */
  onInvalid?(): void;
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
    let live = true;
    let ready = false;
    let saving = false;
    let candidate: { code: string; accelerator: string } | null = null;
    const report = (error: unknown) => latest.current.onError?.(error);
    Promise.resolve(latest.current.onRecording?.(true))
      .then(() => {
        ready = true;
      })
      .catch((error: unknown) => {
        if (live) {
          report(error);
          setRecording(false);
        }
      });
    const key = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (saving) return;
      if (e.key === "Escape") {
        setRecording(false);
        return;
      }
      if (!ready || e.repeat || e.isComposing) return;
      if (e.getModifierState("AltGraph")) {
        latest.current.onInvalid?.();
        return;
      }
      const accelerator = acceleratorFrom(e);
      if (!accelerator) {
        if (needsModifier(e) || !/^(Control|Alt|Shift|Meta)(Left|Right)$/.test(e.code)) latest.current.onInvalid?.();
        return;
      }
      candidate = { code: e.code, accelerator };
    };
    // Restore global registration only after the main key is released, so
    // auto-repeat cannot toggle the island while accepting its own shortcut.
    const released = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (!candidate || candidate.code !== e.code || saving) return;
      saving = true;
      Promise.resolve(latest.current.onChange(candidate.accelerator))
        .catch(report)
        .finally(() => {
          if (live) setRecording(false);
        });
    };
    const away = (e: PointerEvent) => {
      if (!button.current?.contains(e.target as Node)) setRecording(false);
    };
    const blur = () => setRecording(false);
    window.addEventListener("keydown", key, true);
    window.addEventListener("keyup", released, true);
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("blur", blur);
    return () => {
      live = false;
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("keyup", released, true);
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("blur", blur);
      Promise.resolve(latest.current.onRecording?.(false)).catch(report);
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
      {recording ? props.recordingLabel : (props.actionLabel ?? shown)}
    </button>
  );
}
