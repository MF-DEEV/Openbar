// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A session's face in the island: its Clawd, or with buddies off, a round
// badge in the phase's tint (NotchClaudeSessionsView.swift).

import { useLocale } from "../i18n";

import { Hammer, Hourglass, MessageCircleQuestion, MoonStar, type LucideIcon } from "lucide-react";
import type { ComponentType } from "react";
import { phaseTint } from "./format";
import { CheckCircleFill, ClaudeMark, CodexMark, type GlyphProps, HandFill, MARK_SCALE } from "./glyphs";
import type { Phase, Provider } from "./types";

const PHASE_ICON: Record<Phase, LucideIcon | ComponentType<GlyphProps>> = {
  waitingPermission: HandFill,
  waitingInput: MessageCircleQuestion,
  runningTool: Hammer,
  generating: ClaudeMark,
  background: Hourglass,
  finished: CheckCircleFill,
  idle: MoonStar,
};

/** A phase's symbol; generating shows the mark of the agent at work. */
export function phaseIcon(phase: Phase, provider: Provider): LucideIcon | ComponentType<GlyphProps> {
  if (phase === "generating") return provider === "codex" ? CodexMark : ClaudeMark;
  return PHASE_ICON[phase];
}

export function PhaseBadge({ phase, provider, size }: { phase: Phase; provider: Provider; size: number }) {
  useLocale();
  const Icon = phaseIcon(phase, provider);
  const tint = phaseTint(phase, provider);
  const scale = phase === "generating" ? MARK_SCALE[provider] : 1;
  return (
    <span
      className="s-badge"
      style={{ width: size, height: size, background: `color-mix(in srgb, ${tint} 16%, transparent)`, color: tint }}
    >
      <Icon
        size={Math.round(size * 0.42 * scale)}
        strokeWidth={2.5}
        {...(phase === "generating" ? { working: true } : {})}
      />
    </span>
  );
}
