// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// "✻ Pondering…" while a session works (NotchClaudeSessionLargeView.swift
// :432-480; spec §4.11): a star stepping through · ✢ ✳ ✶ ✻ ✽ and back at 8
// steps a second, and a three-letter shimmer crossing the word at 14 letters
// a second. Reduced motion keeps a still ✻. A Codex session shows its own
// mark and color instead of Claude's star.

import { useEffect, useState } from "react";
import { clock, providerTint, thinkingWord } from "./format";
import { CodexMark } from "./glyphs";
import type { Session } from "./types";

const GLYPHS = ["·", "✢", "✳", "✶", "✻", "✽"];

function reduced() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

function useTicker(fps: number, on: boolean): number {
  const [t, setT] = useState(() => performance.now() / 1000);
  useEffect(() => {
    if (!on) return;
    let raf = 0;
    let last = 0;
    const step = (ms: number) => {
      if (ms - last >= 1000 / fps) {
        last = ms;
        setT(ms / 1000);
      }
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [fps, on]);
  return t;
}

/** RGB of the tint (#rrggbb) blended toward white by `amount`. */
function glow(tint: string, amount: number): string {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(tint.slice(i, i + 2), 16)) as [number, number, number];
  const mix = (c: number) => Math.round(c + (255 - c) * amount);
  return `rgb(${mix(r)},${mix(g)},${mix(b)})`;
}

export function ThinkingRow({ session, size, now }: { session: Session; size: number; now: number }) {
  useLocale();
  const still = reduced();
  const t = useTicker(24, !still);
  const word = thinkingWord(session.turnStarted ?? session.phaseSince);
  const step = Math.floor(t * 8) % 10;
  const glyph = still ? "✻" : GLYPHS[step < 6 ? step : 10 - step]!;
  const center = ((t * 14) % (word.length + 12)) - 6;
  const tint = providerTint(session.provider);
  const codex = session.provider === "codex";

  return (
    <div className="s-thinking" style={{ fontSize: size }}>
      <span className="s-thinking-glyph" style={{ color: tint, fontSize: size + 1 }}>
        {codex ? <CodexMark size={size} working={!still} /> : glyph}
      </span>
      <span className="s-thinking-word">
        {[...word].map((ch, i) => (
          <span key={i} style={{ color: still ? tint : glow(tint, Math.max(0, 1 - Math.abs(i - center) / 3) * 0.7) }}>
            {ch}
          </span>
        ))}
      </span>
      <span className="s-tertiary" style={{ fontSize: size - 3 }}>
        {clock(now - (session.turnStarted ?? session.phaseSince))}
      </span>
      {session.phase === "runningTool" && session.currentTool ? (
        <span className="s-tertiary" style={{ fontSize: size - 3 }}>
          {session.currentTool.name}
        </span>
      ) : null}
    </div>
  );
}
