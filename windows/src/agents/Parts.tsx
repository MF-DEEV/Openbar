// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The AI page's small pieces (NotchAgentComponents.swift): an agent's mark,
// the working pulse, chips, the meter with its pace tick, and the card's
// chrome and header.

import { Infinity as InfinityIcon, type LucideIcon, Terminal } from "lucide-react";
import type { ReactNode } from "react";
import { ClaudeMark, CodexMark, MARK_SCALE } from "../sessions/glyphs";
import { PROVIDER_TINTS } from "./format";
import type { Provider } from "./types";

/** OpenCode and Copilot keep a neutral symbol: no vendor artwork ships (spec §10.1). */
const SYMBOLS: Record<"opencode" | "copilot", LucideIcon> = { opencode: Terminal, copilot: InfinityIcon };

/** An agent's mark at the size of the symbol it stands in for (NotchAgentMark). */
export function Mark({ provider, size, tint }: { provider: Provider; size: number; tint?: string }) {
  if (provider === "claude")
    return <ClaudeMark size={Math.round(size * MARK_SCALE.claude)} style={tint ? { color: tint } : undefined} />;
  if (provider === "codex") return <CodexMark size={Math.round(size * MARK_SCALE.codex)} />;
  const Icon = SYMBOLS[provider];
  return <Icon size={size} strokeWidth={2.4} color={tint ?? PROVIDER_TINTS[provider]} aria-hidden />;
}

/** The mark breathing while the agent works, in a frame wide enough for the widest mark (NotchAgentGlyph). */
export function Glyph({ provider, size, working = true }: { provider: Provider; size: number; working?: boolean }) {
  const frame = size * 1.45 + 1;
  return (
    <span className={`ag-glyph${working ? " is-working" : ""}`} style={{ width: frame, height: frame }} aria-hidden>
      <Mark provider={provider} size={size} />
    </span>
  );
}

/** A calm pulse that says an agent is working; still under reduced motion. */
export function Pulse({ tint, size = 5 }: { tint: string; size?: number }) {
  return (
    <span className="ag-pulse" style={{ width: size * 2, height: size * 2, color: tint }} aria-hidden>
      <span style={{ width: size, height: size }} />
      <span className="ag-pulse-ring" style={{ width: size, height: size }} />
    </span>
  );
}

/** 9 pt semibold, tint 90 % on a tint 14 % capsule. */
export function Chip({ children, tint = "#ffffff", title }: { children: ReactNode; tint?: string; title?: string }) {
  return (
    <span className="ag-chip" style={{ color: tint, ["--ag-tint" as string]: tint }} title={title}>
      {children}
    </span>
  );
}

/** A capsule meter with a tick where spending would be if spread evenly across the window. */
export function Meter(props: { value: number; pace?: number | null; tint: string; dimmed?: boolean; height?: number }) {
  const height = props.height ?? 4;
  const fraction = Number.isFinite(props.value) ? Math.min(1, Math.max(0, props.value)) : 0;
  const pace = props.pace;
  return (
    <span className="ag-meter" style={{ height: height + 5 }} aria-hidden>
      <span className="ag-meter-track" style={{ height }} />
      <span
        className="ag-meter-fill"
        style={{
          height,
          width: fraction > 0 ? `max(${height}px, ${fraction * 100}%)` : 0,
          background: props.tint,
          opacity: props.dimmed ? 0.45 : 0.92,
        }}
      />
      {pace != null && Number.isFinite(pace) && pace > 0.02 && pace < 0.98 ? (
        <span className="ag-meter-pace" style={{ left: `calc(${pace * 100}% - 0.75px)`, height: height + 5 }} />
      ) : null}
    </span>
  );
}

/** Padding 12 × 10 on the island's control surface, radius 18. */
export function Card(props: { children: ReactNode; title?: string; className?: string }) {
  return (
    <div className={`ag-card u-surface${props.className ? ` ${props.className}` : ""}`} title={props.title}>
      {props.children}
    </div>
  );
}

/** Icon (or the agent's mark), title, and an accessory on the trailing edge; 17 tall. */
export function CardHeader(props: {
  title: string;
  icon?: LucideIcon;
  provider?: Provider;
  tint?: string;
  children?: ReactNode;
}) {
  const Icon = props.icon;
  return (
    <div className="ag-header">
      {props.provider ? (
        <span className="ag-header-mark">
          <Mark provider={props.provider} size={13} tint={props.tint} />
        </span>
      ) : Icon ? (
        <Icon className="ag-header-icon" size={11} strokeWidth={2.5} color={props.tint} aria-hidden />
      ) : null}
      <span className="ag-header-title">{props.title}</span>
      <span className="ag-header-accessory">{props.children}</span>
    </div>
  );
}
