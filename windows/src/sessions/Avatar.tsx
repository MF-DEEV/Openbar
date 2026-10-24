// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A session's face in the island: its Clawd, or with buddies off, a round
// badge in the phase's tint (NotchClaudeSessionsView.swift).

import { Hammer, Hourglass, MessageCircleQuestion, MoonStar, type LucideIcon } from "lucide-react";
import type { ComponentType } from "react";
import { phaseTint } from "./format";
import { CheckCircleFill, type GlyphProps, HandFill, SparkleFill } from "./glyphs";
import type { Phase } from "./types";

export const PHASE_ICON: Record<Phase, LucideIcon | ComponentType<GlyphProps>> = {
  waitingPermission: HandFill,
  waitingInput: MessageCircleQuestion,
  runningTool: Hammer,
  generating: SparkleFill,
  background: Hourglass,
  finished: CheckCircleFill,
  idle: MoonStar,
};

export function PhaseBadge({ phase, size }: { phase: Phase; size: number }) {
  const Icon = PHASE_ICON[phase];
  const tint = phaseTint(phase);
  return (
    <span
      className="s-badge"
      style={{ width: size, height: size, background: `color-mix(in srgb, ${tint} 16%, transparent)`, color: tint }}
    >
      <Icon size={Math.round(size * 0.42)} strokeWidth={2.5} />
    </span>
  );
}
