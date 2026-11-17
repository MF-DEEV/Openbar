// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, tf, t } from "../i18n";

import { useEffect, useRef } from "react";
import { barHeights, subscribeAudioLevels } from "./audioLevels";
import { useMusic } from "./store";

export function EqualizerBars(props: { count: number; width: number; height: number; color: string; moving: boolean }) {
  useLocale();
  const { count, width, height, color, moving } = props;
  const source = useMusic((state) => state.playback?.source);
  const refs = useRef<(HTMLSpanElement | null)[]>([]);
  const container = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    const media = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    let unsubscribe: (() => void) | undefined;
    const apply = (levels: readonly number[]) => {
      barHeights(count, width, height, levels).forEach((h, i) => {
        const el = refs.current[i];
        if (el) {
          const rising = h > (parseFloat(el.style.height) || width);
          el.style.transition = media?.matches ? "none" : tf("windows.music.heightValueMsEaseOut", rising ? 30 : 80);
          el.style.height = `${h}px`;
        }
      });
    };
    const sync = () => {
      unsubscribe?.();
      unsubscribe = undefined;
      apply([]);
      if (container.current) {
        container.current.dataset.audioCapture = "inactive";
        container.current.title = "";
      }
      if (!moving || !source || document.hidden || media?.matches) return;
      unsubscribe = subscribeAudioLevels((frame) => {
        const matches = frame?.source === source;
        const mode = matches ? frame.mode : "inactive";
        if (container.current) {
          container.current.dataset.audioCapture = mode;
          container.current.title = mode === "system" ? t("windows.music.systemAudioPlayerIsolationUnavailable") : "";
        }
        apply(matches && (mode === "process" || mode === "system") ? frame.levels : []);
      });
    };
    sync();
    document.addEventListener("visibilitychange", sync);
    media?.addEventListener("change", sync);
    return () => {
      unsubscribe?.();
      document.removeEventListener("visibilitychange", sync);
      media?.removeEventListener("change", sync);
    };
  }, [count, width, height, moving, source]);

  return (
    <span ref={container} className="m-bars" style={{ height, gap: width * 0.85 }} aria-hidden>
      {Array.from({ length: count }, (_, i) => (
        <span
          key={i}
          ref={(el) => {
            refs.current[i] = el;
          }}
          style={{ width, height: width, borderRadius: width / 2, background: color }}
        />
      ))}
    </span>
  );
}
