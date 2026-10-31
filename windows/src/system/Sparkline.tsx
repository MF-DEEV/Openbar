// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// A small history graph (Sparkline.swift): a filled area under the line, an
// optional zero baseline, and the graph's ceiling named on a dashed rule
// over the oldest samples (graphCeilingLabel).

import { useId } from "react";

export interface Line {
  values: number[];
  color: string;
  fillOpacity?: number;
}

/** Points in a 100 × `height` box, the newest at the right edge. */
export function sparkPoints(values: number[], max: number, height: number): [number, number][] {
  if (values.length < 2) return [];
  const peak = Math.max(max, 0.0001);
  const top = 0.5;
  const baseline = Math.max(0.5, height - 0.5);
  const plot = Math.max(1, baseline - top);
  return values.map((value, index) => [
    (100 * index) / (values.length - 1),
    baseline - plot * Math.min(1, Math.max(0, value / peak)),
  ]);
}

export function Sparkline(props: { lines: Line[]; max: number; ceilingLabel: string; height?: number }) {
  useLocale();
  const height = props.height ?? 38;
  const id = useId();
  const drawn = props.lines.filter((line) => line.values.length >= 2);
  if (drawn.length === 0) return null;
  const baseline = Math.max(0.5, height - 0.5);
  return (
    <div className="sy-graph" style={{ height }} aria-hidden>
      <svg viewBox={`0 0 100 ${height}`} preserveAspectRatio="none">
        <defs>
          {drawn.map((line, i) => (
            <linearGradient key={i} id={`${id}-${i}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor={line.color} stopOpacity={line.fillOpacity ?? 0.16} />
              <stop offset="1" stopColor={line.color} stopOpacity={0} />
            </linearGradient>
          ))}
        </defs>
        <line className="sy-graph-ceiling" x1="0" y1="0.5" x2="100" y2="0.5" />
        <line className="sy-graph-zero" x1="0" y1={baseline} x2="100" y2={baseline} />
        {drawn.map((line, i) => {
          const points = sparkPoints(line.values, props.max, height);
          const path = points.map(([x, y]) => `${x},${y}`).join(" ");
          const area = `0,${baseline} ${path} 100,${baseline}`;
          return (
            <g key={i}>
              <polygon points={area} fill={`url(#${id}-${i})`} />
              <polyline points={path} fill="none" stroke={line.color} className="sy-graph-line" />
            </g>
          );
        })}
      </svg>
      <span className="sy-graph-label">{props.ceilingLabel}</span>
    </div>
  );
}
