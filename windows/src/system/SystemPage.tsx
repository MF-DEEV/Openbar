// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The System page (NotchSystemView.swift; spec 04 §8): cards at least 128 ×
// 72 in balanced rows, each a label, a rolling value and a meter or a second
// line. A card opens its metric's detail. The grid scrolls only when it is
// taller than the page.

import { useLocale } from "../i18n";

import { BatteryCharging, Gauge } from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { contentBudget } from "../island/geometry";
import { geometryOf, useIsland } from "../island/store";
import type { PageProps } from "../island/views/Expanded";
import { EmptyState } from "../island/views/Controls";
import { RollingDigits } from "../shared/ui/RollingDigits";
import { type Card, systemCards } from "./cards";
import { gridHeight, hoverInset, PAGE_CONTENT_HEIGHT, systemRowRanges } from "./layout";
import { GLYPHS } from "./glyphs";
import { DETAIL_BAR, MetricDetail } from "./MetricDetail";
import { useSystem } from "./store";
import { S } from "./strings";
import "./system.css";

/** The cards for the current reading and the person's switches. */
export function useCards(): Card[] {
  const language = useLocale();
  const snapshot = useSystem((s) => s.snapshot);
  const hidden = useIsland((s) => s.settings.hiddenSystemMetrics);
  const inBits = useIsland((s) => s.settings.networkSpeedUnit === "bits");
  return useMemo(() => systemCards(snapshot, { hidden, inBits }), [snapshot, hidden, inBits, language]);
}

export function SystemPage({ width }: PageProps) {
  useLocale();
  const cards = useCards();
  const snapshot = useSystem((s) => s.snapshot);
  const detail = useSystem((s) => s.detail);
  const budget = useIsland((s) => contentBudget(geometryOf(s), false));
  const custom = useIsland((s) => s.settings.layout === "custom");
  const setPageContent = useIsland((s) => s.setPageContent);
  const grid = gridHeight(cards.length, width);
  // A detail fits its readings, up to a readable page inside a preset
  // (NotchGeometry.pageBudget), plus its back row.
  const [measured, setMeasured] = useState<number | null>(null);
  const pageBudget = custom ? budget : Math.max(budget, PAGE_CONTENT_HEIGHT);
  const detailHeight = DETAIL_BAR + Math.min(pageBudget, measured === null ? pageBudget : Math.ceil(measured));

  useEffect(() => {
    setPageContent("system", detail ? detailHeight : Math.min(budget, grid));
  }, [detail, detailHeight, budget, grid, setPageContent]);
  // No clearing on unmount: during a page transition the outgoing copy
  // unmounts after the incoming one reported, and would undo its height.

  // A metric whose card went away (unplugged, switched off) closes its detail.
  const open = detail !== null && cards.some((c) => c.kind === detail);
  useEffect(() => {
    if (detail !== null && !open) useSystem.getState().closeDetail();
  }, [detail, open]);

  // Nothing known yet (the monitor still probing): an empty page, not "unavailable".
  if (snapshot === null) return null;
  if (detail && open) return <MetricDetail key={detail} kind={detail} onMeasure={setMeasured} />;
  if (cards.length === 0) return <EmptyState icon={Gauge} title={S.unavailable} />;
  const inset = hoverInset(width);
  const rows = systemRowRanges(cards.length, width - inset * 2);
  return (
    <div className="sy-grid" style={{ padding: inset }}>
      {rows.map(([start, end]) => (
        <div key={start} className="sy-row">
          {cards.slice(start, end).map((card) => (
            <CardButton key={card.kind} card={card} />
          ))}
        </div>
      ))}
    </div>
  );
}

function CardButton({ card }: { card: Card }) {
  useLocale();
  const Glyph = card.kind === "battery" && card.charging ? BatteryCharging : GLYPHS[card.kind];
  const value = card.value ?? "…";
  const spoken = [card.value ?? "…", card.detail].filter(Boolean).join(", ");
  return (
    <button
      type="button"
      className={`sy-card${card.detail ? " has-detail" : ""}`}
      aria-label={`${card.title}: ${spoken}`}
      title={card.title}
      onClick={() => useSystem.getState().openDetail(card.kind)}
    >
      <span className="sy-card-label" aria-hidden>
        <Glyph size={11} strokeWidth={2} />
        <span>{card.title}</span>
      </span>
      <FitValue text={value} />
      {card.detail ? (
        <span className="sy-card-detail" aria-hidden>
          {card.detail}
        </span>
      ) : card.level != null ? (
        <Meter value={card.level} attention={card.attention} />
      ) : null}
    </button>
  );
}

/** The value shrinks to fit its card, down to three quarters (minimumScaleFactor 0.75). */
function FitValue({ text }: { text: string }) {
  useLocale();
  const box = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.setProperty("--sy-fit", "1");
    const room = el.parentElement!.clientWidth - 24;
    const need = el.scrollWidth;
    el.style.setProperty("--sy-fit", String(need > room && need > 0 ? Math.max(0.75, room / need) : 1));
  }, [text]);
  return (
    <span ref={box} className="sy-card-value" aria-hidden>
      <RollingDigits text={text} />
    </span>
  );
}

/** A 5 pt capsule: white, or orange when the reading needs attention (NotchMeter). */
export function Meter({ value, attention }: { value: number; attention: boolean }) {
  useLocale();
  const fraction = Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
  return (
    <span className={`sy-meter${attention ? " is-attention" : ""}`} aria-hidden>
      <span style={{ width: fraction > 0 ? `max(5px, ${fraction * 100}%)` : 0 }} />
    </span>
  );
}
