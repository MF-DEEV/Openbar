// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A session's face in the island: its Clawd, or with buddies off, a round
// badge in the phase's tint (NotchClaudeSessionsView.swift).

import {
  Check,
  Hammer,
  Hand,
  Hourglass,
  MessageCircleQuestion,
  MoonStar,
  Sparkle,
  type LucideIcon,
} from "lucide-react";
import { phaseTint } from "./format";
import type { Phase } from "./types";

export const PHASE_ICON: Record<Phase, LucideIcon> = {
  waitingPermission: Hand,
  waitingInput: MessageCircleQuestion,
  runningTool: Hammer,
  generating: Sparkle,
  background: Hourglass,
  finished: Check,
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
