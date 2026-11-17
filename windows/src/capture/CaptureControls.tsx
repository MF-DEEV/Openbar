// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's capture controls (NotchCaptureControlsView): compact at the top
// of the display, clear of what is being captured, open while the pointer rests
// on them. Drawn in the selection window, since that window covers the island.

import { useLocale, t } from "../i18n";

import { Camera, ChevronUp, type LucideIcon, Pipette, ScanText, Video, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { S } from "./strings";
import { TOOLS } from "./tools";
import type { CaptureTool } from "./types";

const ICONS: Record<CaptureTool, LucideIcon> = { screenshot: Camera, recording: Video, text: ScanText, color: Pipette };
const TITLES: Record<CaptureTool, string> = {
  get screenshot() {
    return S.toolScreenshot;
  },
  get recording() {
    return S.toolRecording;
  },
  get text() {
    return S.toolText;
  },
  get color() {
    return S.toolColor;
  },
};
/** How long the open controls wait after the pointer leaves (scheduleCaptureControlsCollapse). */
const COLLAPSE_AFTER_MS = 3000;

export function CaptureControls(props: {
  tool: CaptureTool;
  onTool: (tool: CaptureTool) => void;
  onCancel: () => void;
  offersRepeat: boolean;
  /** A drag is under way: stay small and out of its way. */
  selecting: boolean;
  audio: { system: boolean; microphone: boolean };
  onAudio: (audio: { system: boolean; microphone: boolean }) => void;
}) {
  useLocale();
  const [open, setOpen] = useState(false);
  const inside = useRef(false);
  const focused = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  /** A pointer already resting here when the selection began doesn't open it until it leaves. */
  const heldBack = useRef(true);

  const stopTimer = () => clearTimeout(timer.current);
  const schedule = useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      if (!inside.current && !focused.current) setOpen(false);
    }, COLLAPSE_AFTER_MS);
  }, []);
  useEffect(() => stopTimer, []);
  useEffect(() => {
    const away = (e: PointerEvent) => {
      if (!(e.target instanceof Element) || !e.target.closest(".cc-island")) heldBack.current = false;
    };
    window.addEventListener("pointermove", away);
    return () => window.removeEventListener("pointermove", away);
  }, []);
  useEffect(() => {
    if (props.selecting) setOpen(false);
  }, [props.selecting]);

  const shown = open && !props.selecting;
  const Icon = ICONS[props.tool];
  return (
    <div
      className={`cc-island${shown ? " is-open" : ""}${props.selecting ? " is-passive" : ""}${props.tool === "recording" ? " has-audio" : ""}`}
      role="group"
      aria-label={S.controlsTitle}
      // Nothing here starts an area or picks a colour underneath.
      onPointerDown={(e) => e.stopPropagation()}
      onPointerUp={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.stopPropagation()}
      onPointerEnter={() => {
        inside.current = true;
        stopTimer();
        if (!heldBack.current && !props.selecting) setOpen(true);
      }}
      onPointerLeave={() => {
        inside.current = false;
        heldBack.current = false;
        schedule();
      }}
      onFocus={(e) => {
        if (!e.target.matches(":focus-visible")) return;
        focused.current = true;
        stopTimer();
        setOpen(true);
      }}
      onBlur={() => {
        focused.current = false;
        schedule();
      }}
    >
      {shown ? (
        <div className="cc-body">
          <div className="cc-header">
            <span className="cc-title">{S.controlsTitle}</span>
            <span className="cc-actions">
              {props.offersRepeat ? (
                <span className="cc-key" title={S.repeatRegion}>
                  {t("windows.capture.r")}
                </span>
              ) : null}
              <button
                type="button"
                className="cc-icon"
                aria-label={S.collapse}
                title={S.collapse}
                onClick={() => {
                  (document.activeElement as HTMLElement | null)?.blur();
                  heldBack.current = true;
                  setOpen(false);
                }}
              >
                <ChevronUp size={14} aria-hidden />
              </button>
              <button type="button" className="cc-icon" aria-label={S.close} title={S.close} onClick={props.onCancel}>
                <X size={14} aria-hidden />
              </button>
            </span>
          </div>
          <div className="cc-tools">
            {TOOLS.map(({ id, key }) => {
              const ToolIcon = ICONS[id];
              return (
                <button
                  key={id}
                  type="button"
                  className={`cc-tile${props.tool === id ? " is-active" : ""}`}
                  aria-pressed={props.tool === id}
                  aria-keyshortcuts={key}
                  onClick={(e) => {
                    // A click leaves the keyboard to the selection: Enter still takes the display.
                    if (!e.currentTarget.matches(":focus-visible")) e.currentTarget.blur();
                    props.onTool(id);
                  }}
                >
                  <ToolIcon size={18} aria-hidden />
                  <span>{TITLES[id]}</span>
                </button>
              );
            })}
          </div>
          {props.tool === "recording" ? (
            <div className="cc-audio">
              <label>
                <input
                  type="checkbox"
                  role="switch"
                  checked={props.audio.system}
                  onChange={(e) => props.onAudio({ ...props.audio, system: e.target.checked })}
                />
                {S.systemAudio}
              </label>
              <label>
                <input
                  type="checkbox"
                  role="switch"
                  checked={props.audio.microphone}
                  onChange={(e) => props.onAudio({ ...props.audio, microphone: e.target.checked })}
                />
                {S.microphone}
              </label>
            </div>
          ) : null}
        </div>
      ) : (
        <span className="cc-compact" aria-label={TITLES[props.tool]}>
          <Icon size={14} aria-hidden />
        </span>
      )}
    </div>
  );
}
